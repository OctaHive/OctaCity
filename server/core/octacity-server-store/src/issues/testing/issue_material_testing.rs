//! Material edits use the existing atomic Issue revision publisher.
use super::*;

#[async_trait::async_trait]
impl IssueMaterialStore for crate::testing::InMemoryProjectStore {
  async fn issue_material_edit(
    &self,
    scope: &ManagementSecurityScope,
    key: &IdempotencyKey,
  ) -> Result<Option<IssueMaterialEditReceipt>, StoreError> {
    Ok(
      self
        .lock()?
        .issues
        .material_edits
        .get(&ManagementIdempotencyKey::new(scope.clone(), key.clone()))
        .cloned(),
    )
  }
  async fn edit_issue_materials(
    &self,
    request: ManagementMutation<EditIssueMaterials>,
  ) -> Result<IssueMutationOutcome, StoreError> {
    let (request, audit) = request.into_parts();
    let key = audit.scoped_idempotency_key(&request.idempotency_key);
    let mut state = self.lock()?;
    if let Some(receipt) = state.issues.material_edits.get(&key) {
      if receipt.intent != request.intent {
        return Err(crate::issues::issue_planning::planning_conflict());
      }
      return Ok(IssueMutationOutcome {
        disposition: MutationDisposition::Replayed,
        ..receipt.outcome.clone()
      });
    }
    let current = state
      .issues
      .current
      .get(&(request.intent.project_id, request.intent.id))
      .ok_or(StoreError::NotFound {
        entity: EntityKind::Issue,
      })?;
    let outcome = IssueMutationOutcome {
      disposition: MutationDisposition::Applied,
      issue: current.with_materials(&request, audit.actor())?,
    };
    commit_issue_revision(
      &mut state,
      &audit,
      &key,
      StoreOperation::EditIssueMaterials,
      &outcome.issue,
      None,
      |memory| {
        memory.material_edits.insert(
          key.clone(),
          IssueMaterialEditReceipt {
            intent: request.intent,
            outcome: outcome.clone(),
          },
        );
      },
    )?;
    Ok(outcome)
  }
}
