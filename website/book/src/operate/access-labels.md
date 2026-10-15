<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Labelling the access records

Regulation (EU) 2025/327 asks the components that store personal
electronic health data to "support different retention periods and access
rights that take into account the origins and categories" of the data
(Annex II 3.4). The gateway keeps no access record: each goes to your Audit
Record Repository ([The access log](access-log.md)), which
returns only the records "which the requester is authorized to view" (IHE
RESTful ATNA Rev. 3.6 §3.81.4.1.3), and ATNA asks that "access controls
should appropriately restrict access to audit records" (ITI TF-1 §9.5).
So the access rights are set at your repository, and the gateway gives
each record what the repository needs to set them by: a security label per
category of the data the access reached and per origin of the data, set
when the record is written and sent with it over ITI-20.

## Where a record carries its labels

| Element | Labels |
|---|---|
| `meta.security` | every label of the record, each once: the categories' first, then each origin's in the order the access asked the origins |
| `securityLabel` of the `ehds-categories` entity | the labels of the categories |
| `securityLabel` of each `origin` entity | the labels of that origin |

FHIR R4 `Meta.security` holds the "security labels applied to this
resource", which "connect specific resources to the overall security
policy and infrastructure": a repository that keeps the records as FHIR
resources decides on them. A repository that keeps the DICOM form of a
record keeps each `entity.securityLabel` as that entity's
`ParticipantObjectSensitivity` (IHE RESTful ATNA Rev. 3.6, Table
3.20.4.2.2.1-1), which has no place for `meta`, so the entity labels
survive there.

## The labels a record carries

| The access | Its labels |
|---|---|
| reached data of a category your `[access_log]` map names | that category's labels: the ones `[access_log.labels.categories]` declares for it, or, when it declares none, the category's own code, for a priority category the code of HL7 Europe's `EEHRxFDocumentPriorityCategoryCS` at version `1.0.0-ballot` |
| reached data of no category (`ehds-no-category`) | no category label |
| reached data the map cannot classify (`ehds-unclassified`), in whole or in part | the labels of every category the map can name: the six priority categories and each national category you declare |
| asked an origin (an endpoint of the registry), one that answered nothing included | that origin's labels: the ones `[access_log.labels.origins]` declares for it, or, when it declares none, the endpoint id as the code in `origin_system` |

An unclassified access carries every category's label because nothing says
which categories its data hold: a repository that admits a reader to a
record only when the reader may read each of its labels shows it only to
a reader who may read every category. The same reasoning keeps such a
record for the longest period you declare
([How long a record is kept](access-log.md#how-long-a-record-is-kept)).

No adopted act fixes a label vocabulary for a category or an origin, so
the defaults and the rule for an unclassified access are FerroFED's own
design. The default origin system,
`https://ferrofed.eu/fhir/CodeSystem/access-log-origin`, is a code system
FerroFED owns whose codes are your registry's endpoint ids.

## `[access_log.labels]`

```toml
[access_log.labels]
# the code system an origin with no labels of its own is labelled in, its
# endpoint id the code; FerroFED's own system when unset
origin_system = "https://example.org/fhir/CodeSystem/access-log-origin"

[access_log.labels.categories]
# an Art 14(1) code, or a national <system>|<code> declared in [access_log]
"Laboratory-Reports" = [{ system = "https://example.org/fhir/CodeSystem/access-class", code = "lab" }]

[access_log.labels.origins]
# an endpoint id of the registry
"node-a" = [{ system = "https://example.org/fhir/CodeSystem/access-class", code = "hospital-a" }]
```

A table entry replaces the default label of its category or origin with
the labels it lists, at least one. Each label is a `system`, an absolute
URI without `|`, and a `code`, a FHIR `code`. These are refused when the
configuration loads, naming the key: an `origin_system` that is no
absolute URI, a category the map does not name, two keys that name one
category (`Laboratory-Reports` and its `<system>|<code>` form), an empty
list, a malformed system or code, an unknown key in a label, and an origin
that is no endpoint of the registry. A change is applied by a reload and
reaches the records written after it.

## Configuring your repository

Give each reader of the log the labels they may read, and have the
repository admit a reader to a record only when the reader may read every
label the record carries. For example:

- an operator who reviews the accesses to one member's data is granted
  that member's origin label and every category label;
- a reader who may see accesses to laboratory reports alone is granted the
  `Laboratory-Reports` label and every origin label, and does not see an
  unclassified record, which also carries the other categories' labels;
- your Member State's electronic health data access service, which serves
  each person all accesses to their own data (Art 9(2)), is granted every
  label.

ATNA suggests the IUA Profile for the authorization: the repository
grouped with an IUA Resource Server enforces the policy, and the Audit
Consumer grouped with an IUA Authorization Client presents the reader's
authorization (ITI TF-1 §9.5). ITI-81 defines no behaviour for the FHIR
search parameters beyond its own (§3.81.4.1.2.2), so whether a reader can
also search the log by label is your repository's choice.

A record carries the labels it was written with. After a change to
`[access_log.labels]`, the records written before it keep their earlier
labels, so review your repository's grants against both.
