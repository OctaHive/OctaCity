import type { components } from '../../.generated/api/schema';
import { issueFixture } from './issues';

type Schemas = components['schemas'];
export function reviewFixture(project = 'delivery'): Schemas['IssueCodeReviewProjection'] {
  const pin = (identity: string) => ({ identity, version: 'v1', digest: '01'.repeat(32) });
  const selection = {
    source: {
      installation: pin('independent-forge'),
      capability: pin('reviews'),
      configuration: pin('review-profile'),
      resource: pin('code'),
    },
    item: { resource: 'native-code', item: 'native-mr-42' },
  };
  return {
    association: {
      id: 'ab'.repeat(32),
      sequence: 1,
      project_id: project,
      issue_id: issueFixture(project).id,
      selection,
      binding: null,
      provenance: {
        kind: 'explicit',
        actor: { kind: 'authenticated_management', identity: 'operator-one' },
      },
      attached_at: 1700000000000,
    },
    observation: {
      selection,
      observed_at: 1700000000000,
      title: 'Preserve manual tasks <script>',
      number: 'MR-42',
      url: 'https://forge.example/team/code/reviews/42',
      author: 'Visible author',
      state: 'open',
      draft: false,
      head: 'head-two',
      base: 'base-one',
      changed_files: 3,
      additions: 12,
      deletions: 4,
      review: 'Review pending',
      checks: '2 external checks passed',
    },
    availability: 'available',
  };
}
export function reviewPage(items = [reviewFixture()]): Schemas['IssueCodeReviewPage'] {
  return { items, next_cursor: null, observation: 'issue-code-review-membership-v1' };
}

/** Existing authorized connection/resource vocabulary, independent of the ticket provider. */
export function reviewConnectionFixture(): Schemas['IntegrationConnection'] {
  const source = reviewFixture().association.selection.source;
  return {
    id: '0199a6f4-d56c-7440-9aa2-6a320f862795',
    version: 2,
    digest: '02'.repeat(32),
    published_at_unix_ms: 1,
    definition: {
      name: 'Code forge',
      provider: 'fake-mr',
      installation: source.installation,
      adapter: source.installation,
      authentication: 'installed',
      endpoint: 'https://forge.example',
      resources: [],
    },
  };
}
export function reviewResourcesFixture(): Schemas['IntegrationProjectResources'] {
  const source = reviewFixture().association.selection.source;
  return {
    resources: [
      {
        reference: source.resource,
        native_id: 'native-code',
        locator: 'team/code',
        capabilities: [
          {
            purpose: 'repository_read',
            capability: source.capability,
            configuration: source.configuration,
          },
        ],
      },
    ],
  };
}

export function reviewAccessFixture(
  project = 'delivery',
): Schemas['IntegrationProjectAccessState'] {
  const connection = reviewConnectionFixture();
  return {
    access: {
      connection_id: connection.id,
      connection_version: connection.version,
      version: 1,
      project_id: project,
      published_at_unix_ms: 1,
      resources: [
        {
          reference: reviewFixture().association.selection.source.resource,
          purposes: ['repository_read'],
        },
      ],
    },
  };
}
