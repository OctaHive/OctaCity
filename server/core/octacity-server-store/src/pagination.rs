use std::num::NonZeroU16;

pub(crate) fn finish_bounded_page<T, C: Copy>(
  items: &mut Vec<T>,
  limit: NonZeroU16,
  cursor_of: impl Fn(&T) -> C,
) -> Option<C> {
  let limit = usize::from(limit.get());
  let has_more = items.len() > limit;
  items.truncate(limit);
  has_more.then(|| cursor_of(items.last().expect("a non-zero full page has a last item")))
}
