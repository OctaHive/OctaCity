use octacity_server_application::{
  ManagementAuthorizationGrant, ManagementQueryUseCase, ManagementRequestContext, ManagementRequestId,
  ManagementVisibility, Query,
};
use uuid::Uuid;

pub async fn management_query<Q, H>(handler: &H, query: Q) -> Result<Q::Outcome, H::Error>
where
  Q: Query,
  H: ManagementQueryUseCase<Q>,
{
  let context = ManagementRequestContext::trusted_network(
    ManagementRequestId::new(Uuid::from_u128(0x74eb_362d_4264_4d3a_88b8_5569_74a3_b017)).unwrap(),
  );
  let grant = ManagementAuthorizationGrant::new(ManagementVisibility::all());
  handler.execute_management_query(&context, &grant, query).await
}
