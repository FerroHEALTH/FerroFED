- A security patch release cut on an older supported line no longer becomes
  the repository's latest release or moves the `latest` tag of the gateway
  and console images: only the highest version released does. The release
  checklist describes how a security patch is cut from a supported line's
  last tag on a `release/<major>.<minor>.x` maintenance branch (#1087).
