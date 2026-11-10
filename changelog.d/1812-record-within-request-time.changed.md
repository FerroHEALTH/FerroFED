- The record of an access may take what is left of the request's own time
  to be stored, `server.request_timeout` less the one-second combining
  margin, up to `[audit.repository] spool_write_timeout`, whose default
  rises from 2 s to 10 s. Under load a durable write to the spool can wait
  seconds for the disk, and a read is no longer refused `503
  access-unrecorded` while the request still has time; past it the answer
  is still withheld (#1812).
