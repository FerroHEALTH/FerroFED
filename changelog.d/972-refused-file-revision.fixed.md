- Under `config.ui = "managed"`, a `SIGHUP` reload whose changed file the
  reload refuses records the file's revision and a `config-revision-refused`
  record naming the class of the refusal, and leaves the running mark where
  it was, so no other replica on the store follows it. The revision is
  marked running only once the reload applied it, and the same file read
  again is tried again (#972).
