use octacity_server_store::ReadVisibilityView;

/// Parameterized PostgreSQL representation of one typed read-visibility scope.
pub(crate) struct SqlReadVisibility {
  /// Whether the query may select every otherwise matching row.
  pub(crate) all: bool,
  /// Exact visible identities used by `ANY($n::uuid[])` for restricted scopes.
  pub(crate) identities: Vec<uuid::Uuid>,
}

/// Converts a backend-neutral scope without interpolating identities into SQL.
pub(crate) fn sql_read_visibility<I>(
  visibility: ReadVisibilityView<'_, I>,
  as_uuid: impl Fn(&I) -> uuid::Uuid,
) -> SqlReadVisibility {
  match visibility {
    ReadVisibilityView::All => SqlReadVisibility {
      all: true,
      identities: Vec::new(),
    },
    ReadVisibilityView::None => SqlReadVisibility {
      all: false,
      identities: Vec::new(),
    },
    ReadVisibilityView::Restricted(identities) => SqlReadVisibility {
      all: false,
      identities: identities.iter().map(as_uuid).collect(),
    },
  }
}
