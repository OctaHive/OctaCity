## ADDED Requirements

### Requirement: REST exposes authorized Issue summaries for the Overview

The API SHALL provide an additive, bounded read of current summaries for an exact Project and Issue. Each item SHALL return its slot and presentation type, supported conclusion and fields, authorized resource references and exact original Run reference. Triage responses SHALL distinguish a recommendation from a confirmed change to Issue planning. A missing or inaccessible result SHALL use the established visibility and availability semantics. Reads SHALL use retained OctaCity data and perform no mutation, provider call or model invocation. The generated contract SHALL describe the supported variants and availability.

#### Scenario: A caller lacks access to one linked document

- **WHEN** a visible summary refers to a document the caller cannot read
- **THEN** the response contains neither that document's identity nor its title or link

#### Scenario: A triage result is accepted and the planning update conflicts

- **WHEN** the authorized caller loads the Overview
- **THEN** the API returns the assessment with a conflict outcome and does not claim that priority or size was applied
