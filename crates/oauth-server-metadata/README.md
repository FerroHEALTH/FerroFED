<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# oauth-server-metadata

OAuth 2.0 Authorization Server Metadata (RFC 8414) in Rust: what a client
holds an authorization server's metadata to before it sends that server
anything.

- `Issuer`: an issuer identifier, an `http` or `https` URL without
  userinfo, query or fragment, in the canonical form the URL parser gives it
  (RFC 8414 §2).
- `Issuer::metadata_url`: the well-known metadata URL, the suffix inserted
  between the issuer's host and its path (RFC 8414 §3.1).
- `Issuer::is_identical_to`: the metadata's `issuer` is identical to the
  issuer the client asked (RFC 8414 §3.3).
- `Issuer::endpoint`: an endpoint the client sends to is on the issuer's
  origin, without userinfo or a fragment.
- `repeats_no_name`: an answer whose objects repeat a name at any depth is
  refused before it is read.

Which metadata members a profile reads beyond these, and what it requires of
them, is the profile's own: the crate reads no member but through the caller.
The crate depends on no application.

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use and
for non-commercial production use; a commercial licence for other production
use; Apache License 2.0 four years after each version.
