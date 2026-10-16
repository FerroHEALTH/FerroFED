- The operator console page of the book shows every console screen in the
  light theme, with a link to the same screen in the dark theme: the sign-in,
  the dashboard, the members and a member's page, the routing table, the
  integrity incidents, the query console empty and with a complete, an
  incomplete and a refused answer, the stored queries and one query's
  detail, the self-description, each configuration tab and the member form,
  the About page and the page for an unknown path (#1081). The testkit takes
  them behind `FERROFED_SCREENSHOTS=1`, and a test fails when a console
  route has no screenshot.
