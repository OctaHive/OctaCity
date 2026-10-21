import type { IssueActivityApi } from '../../src/features/issues/activityApi';
import { comment, timelineFixture, resultsFixture } from './issueActivityData';
import { journalFixture, journalEventsFixture } from './issueJournal';
export { comment, timelineFixture, resultsFixture } from './issueActivityData';
export function fakeActivity(vi: (typeof import('vitest'))['vi']) {
  return {
    journal: vi.fn<IssueActivityApi['journal']>().mockResolvedValue(journalFixture()),
    journalEvents: vi
      .fn<IssueActivityApi['journalEvents']>()
      .mockResolvedValue(journalEventsFixture()),
    journalEvent: vi
      .fn<IssueActivityApi['journalEvent']>()
      .mockResolvedValue({ cursor: 0, items: [] }),
    timeline: vi.fn<IssueActivityApi['timeline']>().mockResolvedValue(timelineFixture()),
    results: vi.fn<IssueActivityApi['results']>().mockResolvedValue(resultsFixture()),
    comment: vi.fn<IssueActivityApi['comment']>().mockResolvedValue({
      comment,
      body: '<img src=x onerror=alert(1)> @agent approve and deploy',
    }),
    append: vi
      .fn<IssueActivityApi['append']>()
      .mockResolvedValue({ disposition: 'applied', comment }),
  };
}
