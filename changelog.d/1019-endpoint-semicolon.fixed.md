- The configuration page's member form writes a semicolon inside an
  endpoint as `%3B` and reads it back once, so a member whose endpoint URL,
  identifier system or profile holds a `;` saves back through the edit
  form. Before, the form split such an endpoint at the semicolon and
  refused it.
