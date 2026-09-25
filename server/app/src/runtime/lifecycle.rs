use tokio::task::JoinHandle;
use tracing::info;

use super::{DurableWorkerError, ListenerTaskResult, ServerRuntime, ServerRuntimeError};

enum RuntimeTaskExit {
  Listener(Option<Result<ListenerTaskResult, tokio::task::JoinError>>),
  Readiness(Result<(), tokio::task::JoinError>),
  Workers(Result<Result<(), DurableWorkerError>, tokio::task::JoinError>),
  Notifications(Result<(), tokio::task::JoinError>),
  Metrics(Result<(), tokio::task::JoinError>),
}

impl ServerRuntime {
  /// Waits until any supervised process task exits unexpectedly.
  ///
  /// Process entry points should select this future against their shutdown
  /// signal so a failed listener or background worker cannot leave an
  /// apparently healthy process.
  pub async fn wait(&mut self) -> Result<(), ServerRuntimeError> {
    let exit = tokio::select! {
      result = self.listener_tasks.join_next() => RuntimeTaskExit::Listener(result),
      result = wait_for_optional_task(&mut self.readiness_task) => RuntimeTaskExit::Readiness(result),
      result = wait_for_optional_task(&mut self.worker_task) => RuntimeTaskExit::Workers(result),
      result = wait_for_optional_task(&mut self.notification_task) => RuntimeTaskExit::Notifications(result),
      result = wait_for_optional_task(&mut self.metrics_task) => RuntimeTaskExit::Metrics(result),
    };
    let failure = match exit {
      RuntimeTaskExit::Listener(result) => match result {
        Some(Ok(Ok(ingress))) => ServerRuntimeError::ListenerUnexpectedExit { ingress },
        Some(Ok(Err((ingress, source)))) => ServerRuntimeError::Serve { ingress, source },
        Some(Err(source)) => ServerRuntimeError::ListenerTask(source),
        None => ServerRuntimeError::UnexpectedExit,
      },
      RuntimeTaskExit::Readiness(result) => {
        self.readiness_task.take();
        result.map_or_else(ServerRuntimeError::ReadinessTask, |()| {
          ServerRuntimeError::SupervisedTaskUnexpectedExit {
            task: "readiness-monitor",
          }
        })
      }
      RuntimeTaskExit::Workers(result) => {
        self.worker_task.take();
        match result {
          Err(source) => ServerRuntimeError::WorkerTask(source),
          Ok(Err(source)) => ServerRuntimeError::DurableWorker(source),
          Ok(Ok(())) => ServerRuntimeError::SupervisedTaskUnexpectedExit {
            task: "durable-workers",
          },
        }
      }
      RuntimeTaskExit::Notifications(result) => {
        self.notification_task.take();
        result.map_or_else(ServerRuntimeError::NotificationTask, |()| {
          ServerRuntimeError::SupervisedTaskUnexpectedExit {
            task: "ready-job-notifications",
          }
        })
      }
      RuntimeTaskExit::Metrics(result) => {
        self.metrics_task.take();
        result.map_or_else(ServerRuntimeError::MetricsTask, |()| {
          ServerRuntimeError::SupervisedTaskUnexpectedExit { task: "metrics-upkeep" }
        })
      }
    };
    self.readiness.set(false);
    self.cancellation.cancel();
    while self.listener_tasks.join_next().await.is_some() {}
    if let Some(readiness_task) = self.readiness_task.take()
      && let Err(source) = readiness_task.await
    {
      return Err(ServerRuntimeError::ReadinessTask(source));
    }
    if let Some(worker_task) = self.worker_task.take() {
      match worker_task.await {
        Err(source) => return Err(ServerRuntimeError::WorkerTask(source)),
        Ok(Err(source)) => return Err(ServerRuntimeError::DurableWorker(source)),
        Ok(Ok(())) => {}
      }
    }
    if let Some(notification_task) = self.notification_task.take()
      && let Err(source) = notification_task.await
    {
      return Err(ServerRuntimeError::NotificationTask(source));
    }
    if let Some(metrics_task) = self.metrics_task.take()
      && let Err(source) = metrics_task.await
    {
      return Err(ServerRuntimeError::MetricsTask(source));
    }
    Err(failure)
  }

  /// Stops admission, cancels the process tree, and waits for bounded drain.
  pub async fn shutdown(mut self) -> Result<(), ServerRuntimeError> {
    self.readiness.set(false);
    self.cancellation.cancel();
    if self.listener_tasks.is_empty() {
      return Err(ServerRuntimeError::UnexpectedExit);
    }
    let Some(mut readiness_task) = self.readiness_task.take() else {
      return Err(ServerRuntimeError::UnexpectedExit);
    };
    let mut worker_task = self.worker_task.take();
    let mut notification_task = self.notification_task.take();
    let mut metrics_task = self.metrics_task.take();
    let result = tokio::time::timeout(self.shutdown_grace, async {
      let mut listener_error = None;
      while let Some(result) = self.listener_tasks.join_next().await {
        match result {
          Ok(Ok(_)) => {}
          Ok(Err((ingress, source))) if listener_error.is_none() => {
            listener_error = Some(ServerRuntimeError::Serve { ingress, source });
          }
          Err(source) if listener_error.is_none() => {
            listener_error = Some(ServerRuntimeError::ListenerTask(source));
          }
          _ => {}
        }
      }
      if let Err(source) = (&mut readiness_task).await {
        return Err(ServerRuntimeError::ReadinessTask(source));
      }
      if let Some(task) = worker_task.as_mut() {
        match task.await {
          Err(source) => return Err(ServerRuntimeError::WorkerTask(source)),
          Ok(Err(source)) => return Err(ServerRuntimeError::DurableWorker(source)),
          Ok(Ok(())) => {}
        }
      }
      if let Some(task) = notification_task.as_mut()
        && let Err(source) = task.await
      {
        return Err(ServerRuntimeError::NotificationTask(source));
      }
      if let Some(task) = metrics_task.as_mut()
        && let Err(source) = task.await
      {
        return Err(ServerRuntimeError::MetricsTask(source));
      }
      listener_error.map_or(Ok(()), Err)
    })
    .await;
    match result {
      Ok(Ok(())) => {
        info!(%self.management_addr, "server shutdown complete");
        Ok(())
      }
      Ok(Err(error)) => Err(error),
      Err(_) => {
        self.listener_tasks.abort_all();
        readiness_task.abort();
        if let Some(task) = worker_task.as_mut() {
          task.abort();
        }
        if let Some(task) = notification_task.as_mut() {
          task.abort();
        }
        if let Some(task) = metrics_task.as_mut() {
          task.abort();
        }
        while self.listener_tasks.join_next().await.is_some() {}
        let _ = readiness_task.await;
        if let Some(task) = worker_task {
          let _ = task.await;
        }
        if let Some(task) = notification_task {
          let _ = task.await;
        }
        if let Some(task) = metrics_task {
          let _ = task.await;
        }
        Err(ServerRuntimeError::ShutdownTimeout(self.shutdown_grace))
      }
    }
  }
}

impl Drop for ServerRuntime {
  fn drop(&mut self) {
    self.readiness.set(false);
    self.cancellation.cancel();
    self.listener_tasks.abort_all();
    if let Some(readiness_task) = self.readiness_task.take() {
      readiness_task.abort();
    }
    if let Some(worker_task) = self.worker_task.take() {
      worker_task.abort();
    }
    if let Some(notification_task) = self.notification_task.take() {
      notification_task.abort();
    }
    if let Some(metrics_task) = self.metrics_task.take() {
      metrics_task.abort();
    }
  }
}

async fn wait_for_optional_task<T>(task: &mut Option<JoinHandle<T>>) -> Result<T, tokio::task::JoinError> {
  match task {
    Some(task) => task.await,
    None => std::future::pending().await,
  }
}
