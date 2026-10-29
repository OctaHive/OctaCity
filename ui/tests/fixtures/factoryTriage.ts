import type { components } from '../../.generated/api/schema';
type Completion = components['schemas']['FactoryNodeCompletionDiagnostic'];

/** Public gate projections; private reasoning and agent inputs are intentionally absent. */
export function triageCompletion(stage: 'eligibility' | 'classification'): Completion {
  return {
    result_provenance: [
      {
        node: {
          id: `original-${stage}-agent-attempt`,
          factory_run_id: '44444444-4444-4444-8444-444444444444',
          flow_run_id: 'source-flow',
          cycle_id: 'source-cycle',
          node_key: `source-${stage}`,
          node_kind: 'reasoning',
          number: 1,
          input_digest: 'a'.repeat(64),
          execution: {
            kind: 'external',
            identity: { identity: 'native-agent', version: 'v1', digest: 'a'.repeat(64) },
          },
          started_at: 1,
          deadline: 100,
          retry_of: null,
        },
        profile: {
          selected_profile: {
            project_id: '11111111-1111-4111-8111-111111111111',
            id: `original-${stage}-profile`,
            version: 7,
            digest: 'e'.repeat(64),
          },
          operation_id: 'b'.repeat(64),
          context_id: `original-${stage}-context`,
          context_digest: 'c'.repeat(64),
          tool: { identity: 'native-agent', version: 'v1', digest: 'a'.repeat(64) },
          plugin: { identity: 'runner', version: 'v1', digest: 'b'.repeat(64) },
          model_or_tool: { identity: 'selected-model', version: 'v1', digest: 'c'.repeat(64) },
          task_digest: 'd'.repeat(64),
          permission_digest: 'e'.repeat(64),
          build_configuration: {
            id: '55555555-5555-4555-8555-555555555555',
            project_id: '11111111-1111-4111-8111-111111111111',
            version: 3,
            definition_digest: 'f'.repeat(64),
          },
        },
      },
    ],
    id: `${stage}-record`,
    node_attempt_id: `${stage}-attempt`,
    flow_run_id: 'original-flow',
    cycle_id: 'original-cycle',
    node_key: `configured-${stage}`,
    outcome: stage === 'eligibility' ? 'review' : 'accepted',
    output_schema: {
      identity: `factory.triage.${stage === 'eligibility' ? 'eligibility_accepted' : 'classification_summary'}`,
      version: 'v1',
      digest: 'a'.repeat(64),
    },
    output_digest: 'b'.repeat(64),
    observed_at: 42,
    usage: { attempts: 1, tokens: 0, cost_micro_units: 0, elapsed_millis: 0, output_bytes: 0 },
    public_result:
      stage === 'eligibility'
        ? {
            accepted: false,
            decision: 'review',
            reason: 'Scope needs an operator decision',
            evidence: [
              {
                traceArtifactRef: 'retained-trace',
                sequence: 4,
                call_id: 'lookup-call',
                result_digest: 'c'.repeat(64),
              },
            ],
            lookup: {
              status: 'needs_review',
              capture_status: 'complete',
              coverage: 'unknown',
              observations: [
                {
                  reference: {
                    traceArtifactRef: 'retained-trace',
                    sequence: 4,
                    call_id: 'lookup-call',
                    result_digest: 'c'.repeat(64),
                  },
                  connection: 'scoped-project-issues',
                  configuration: { identity: 'issues.read', version: 'v1', digest: 'd'.repeat(64) },
                  resource: {
                    identity: 'only-this-project',
                    version: 'v1',
                    digest: 'e'.repeat(64),
                  },
                  tool: 'search_issues',
                  status: 'completed',
                  coverage: 'unknown',
                  arguments_digest: 'f'.repeat(64),
                },
              ],
            },
            claims: 'hidden-candidate-payload',
            credentials: 'hidden-token',
          }
        : {
            accepted: true,
            admission_priority: 25,
            admission_risk: 'low',
            assessment: {
              priority_assessment: 80,
              work_kind: 'defect',
              severity: 'high',
              size: 'small',
              component: 'server',
              risk: 'medium',
              dependencies: [],
              recommended_route: 'development',
              reproducibility: 'needs_research',
            },
            evidence: 'absent',
            context: 'hidden-context',
            reasoning: 'hidden-reasoning',
          },
  };
}
