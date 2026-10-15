- The operator console draws its navigation and the Sign out button only for a
  signed-in operator, decided by the server from the session; a visitor's
  landing page shows what the console is and the sign-in. When the session
  store cannot be read, a page answers 500 and says so, offering neither
  (#1036).
