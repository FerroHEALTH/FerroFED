- The end-to-end harness's SanteMPI no longer fails a run on SanteMPI's
  transient `IOException` from its publish-and-subscribe queue. Provisioning
  a security application or an identity domain, and an ITI-93 feed, are
  tried again a bounded number of times with a backoff. A record SanteMPI
  already holds is not created again, and any other refusal still fails at
  once (#1302).
