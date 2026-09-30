use octacity_server_application::{
  Command, ManagementAuthorizationGrant, ManagementCommandUseCase, ManagementRequestContext, ManagementRequestId,
  ManagementVisibility,
};
use uuid::Uuid;

pub async fn management_command<C, H>(handler: &H, command: C) -> Result<C::Outcome, H::Error>
where
  C: Command,
  H: ManagementCommandUseCase<C>,
{
  let context = ManagementRequestContext::trusted_network(
    ManagementRequestId::new(Uuid::from_u128(0x74eb_362d_4264_4d3a_88b8_5569_74a3_b017)).unwrap(),
  );
  let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
  handler.execute_management_command(&context, &grant, command).await
}
