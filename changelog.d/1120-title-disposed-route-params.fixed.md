- The operator console's page of one member and the detail of a stored
  query no longer fail now and then with a cut-off answer. Each page's
  title read the route's parameters after the server had cleaned them up,
  which panicked the render; the title now falls back to the name the page
  rendered with. A test fetches the stored query's detail fifty times in a
  row and checks its title each time.
