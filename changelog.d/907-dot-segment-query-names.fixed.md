- A stored query named `.` or `..` with no namespace is refused with
  `400 query-name-invalid`, on `PUT`, `GET` and a run by name, spelled
  plainly or percent-encoded. Such a name is an RFC 3986 dot-segment, so its
  URL resolved to another resource and the gateway could not name it in a
  `Location` or reach it at a node. A name with a namespace, or of three or
  more dots, is no dot-segment and is still accepted (#907).
