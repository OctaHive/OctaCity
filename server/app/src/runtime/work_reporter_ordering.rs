//! Native steps are guarded by accepted source-item ordering before every write.
use super::*;
use octacity_server_store::{
  SourceProjectionAction, SourceProjectionRequest, SourceProjectionState, WorkSourceFailure,
};

impl<S: FactorySourceStore + FactoryRunStore> HostedFactoryWorkStatusProjector<S> {
  async fn order(
    &self,
    request: &FactoryWorkStatusRequest,
    fields: &[FactoryDigest],
    action: SourceProjectionAction,
    end: tokio::time::Instant,
  ) -> Result<SourceProjectionState, ApplicationError> {
    tokio::time::timeout_at(
      end,
      self.store.coordinate_source_projection(SourceProjectionRequest {
        project_id: request.subject.project_id(),
        work_id: request.work_id,
        operation_id: request.operation_id,
        claim: request.outbox_claim.ok_or_else(ApplicationError::invalid)?,
        observed_at: self.clock.now()?,
        fields: fields.to_vec(),
        action,
      }),
    )
    .await
    .map_err(|_| ApplicationError::unavailable())?
    .map_err(Into::into)
  }
  pub(super) async fn project_ordered(
    &self,
    request: &FactoryWorkStatusRequest,
    profile: &FactoryReporterProfile,
    intent: reporter::ProjectionIntent,
    started: tokio::time::Instant,
    end: tokio::time::Instant,
  ) -> Result<(FactoryDigest, BudgetUsage), ApplicationError> {
    let (metadata, mut output) = self.invoke(profile, intent.clone(), None, end).await?;
    let fields = field_ids(&metadata.fields)?;
    measured_usage(started, output)?
      .validate(request.budget)
      .map_err(|_| ApplicationError::invalid())?;
    match self
      .order(request, &fields, SourceProjectionAction::Register, end)
      .await?
    {
      SourceProjectionState::Ready { .. } => {}
      state => return Err(order_error(state)),
    }
    // The original deadline, action output budget and outbox retry policy also
    // bound this ceiling. Neither a new observation nor a lease renews them.
    for _ in 0..256 {
      let before = self
        .order(
          request,
          &fields,
          SourceProjectionAction::Observe { revision: None },
          end,
        )
        .await?;
      let SourceProjectionState::Observation { revision, .. } = before else {
        return Err(order_error(before));
      };
      let (plan, bytes) = self.invoke(profile, intent.clone(), None, end).await?;
      output = output.saturating_add(bytes);
      measured_usage(started, output)?
        .validate(request.budget)
        .map_err(|_| ApplicationError::invalid())?;
      if field_ids(&plan.fields)? != fields {
        return Err(conflict());
      }
      let after = self
        .order(
          request,
          &fields,
          SourceProjectionAction::Observe {
            revision: Some(revision),
          },
          end,
        )
        .await?;
      let SourceProjectionState::Observation {
        stable,
        fields: owned,
        quiescent,
        ..
      } = after
      else {
        return Err(order_error(after));
      };
      if !stable {
        continue;
      }
      if plan.receipt.matches {
        if owned != fields {
          return Err(conflict());
        }
        if !quiescent {
          tokio::time::timeout_at(end, tokio::time::sleep(Duration::from_millis(10)))
            .await
            .map_err(|_| ApplicationError::unavailable())?;
          continue;
        }
        if self.clock.now()? >= request.deadline {
          return Err(ApplicationError::invalid());
        }
        let receipt = serde_json::to_vec(&plan.receipt).map_err(|_| ApplicationError::invalid())?;
        let usage = measured_usage(started, output)?;
        usage
          .validate(request.budget)
          .map_err(|_| ApplicationError::invalid())?;
        return Ok((
          FactoryDigest::sha256("octacity.factory.reporter-receipt.v1", &[&receipt]),
          usage,
        ));
      }
      let step = plan
        .steps
        .into_iter()
        .find(|step| field_ids(&step.fields).is_ok_and(|fields| fields.iter().all(|f| owned.contains(f))))
        .ok_or_else(conflict)?;
      // Revalidate the original accepted action/claim as well as item-wide order.
      tokio::time::timeout_at(end, self.material(request))
        .await
        .map_err(|_| ApplicationError::unavailable())??;
      let step_bytes = serde_json::to_vec(&step).map_err(|_| ApplicationError::invalid())?;
      let step_id = FactoryDigest::sha256(
        "octacity.factory.reporter-dispatch.v1",
        &[&step_bytes, uuid::Uuid::new_v4().as_bytes()],
      );
      match self
        .order(
          request,
          &fields,
          SourceProjectionAction::Begin {
            step_id,
            fields: field_ids(&step.fields)?,
          },
          end,
        )
        .await?
      {
        SourceProjectionState::Granted { .. } => {}
        state => return Err(order_error(state)),
      }
      let result = self.invoke(profile, intent.clone(), Some(step), end).await;
      // Cleanup may report an already dispatched write after its old claim has
      // expired. It never grants new effects and has its own bounded grace.
      let finished = self
        .order(
          request,
          &fields,
          SourceProjectionAction::Finish {
            step_id,
            uncertain: result.as_ref().is_err_and(|failure| failure.uncertain),
          },
          end.max(tokio::time::Instant::now() + self.grace),
        )
        .await;
      let (progress, bytes) = result?;
      finished?;
      if field_ids(&progress.fields)? != fields {
        return Err(conflict());
      }
      output = output.saturating_add(bytes);
      measured_usage(started, output)?
        .validate(request.budget)
        .map_err(|_| ApplicationError::invalid())?;
    }
    Err(ApplicationError::unavailable())
  }
}
fn field_ids(fields: &[String]) -> Result<Vec<FactoryDigest>, ApplicationError> {
  let mut result = fields
    .iter()
    .map(|f| f.parse().map_err(|_| ApplicationError::invalid()))
    .collect::<Result<Vec<_>, _>>()?;
  result.sort();
  Ok(result)
}
fn conflict() -> ApplicationError {
  ApplicationError::SourceDependency {
    class: WorkSourceFailure::Conflict,
    retry_after_millis: None,
  }
}
fn order_error(state: SourceProjectionState) -> ApplicationError {
  match state {
    SourceProjectionState::Blocked => ApplicationError::unavailable(),
    _ => conflict(),
  }
}
