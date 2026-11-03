- The operator console serves the session watch, `GET /session` and
  `POST /session`, which answer the idle and absolute lifetime the session
  has left and `401` once it is gone, so a page warns before the idle
  timeout and every tab ends together (#1505).
