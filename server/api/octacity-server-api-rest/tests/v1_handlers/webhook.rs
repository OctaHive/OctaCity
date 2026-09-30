use super::*;

#[async_trait]
impl ManagementCommandUseCase<CreateManagedWebhookCommand> for RecordingApplication {
  type Error = ApplicationError;

  async fn execute_management_command(
    &self,
    _context: &ManagementRequestContext,
    _grant: &ManagementAuthorizationGrant,
    _command: CreateManagedWebhookCommand,
  ) -> Result<<CreateManagedWebhookCommand as Command>::Outcome, Self::Error> {
    self.record("create_managed_webhook");
    managed_webhook_unavailable(self)
  }
}

macro_rules! unavailable_managed_webhook_command {
  ($command:ty, $call:literal) => {
    #[async_trait]
    impl ManagementCommandUseCase<$command> for RecordingApplication {
      type Error = ApplicationError;

      async fn execute_management_command(
        &self,
        _context: &ManagementRequestContext,
        _grant: &ManagementAuthorizationGrant,
        _command: $command,
      ) -> Result<<$command as Command>::Outcome, Self::Error> {
        self.record($call);
        managed_webhook_unavailable(self)
      }
    }
  };
}

unavailable_managed_webhook_command!(ObserveManagedWebhookRegistrationCommand, "observe_managed_webhook");
unavailable_managed_webhook_command!(RotateManagedWebhookRegistrationCommand, "rotate_managed_webhook");
unavailable_managed_webhook_command!(DeleteManagedWebhookRegistrationCommand, "delete_managed_webhook");

fn managed_webhook_unavailable(
  application: &RecordingApplication,
) -> Result<octacity_server_application::ManagedWebhookProjection, ApplicationError> {
  if application.capability_unavailable_for_managed {
    Err(ApplicationError::capability_unavailable())
  } else {
    Err(ApplicationError::unavailable())
  }
}
