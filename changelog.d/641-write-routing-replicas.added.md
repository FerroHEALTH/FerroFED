- The client contract and the replicas section say how a follow-up write
  behaves behind several gateway replicas (#641): each replica holds its
  own resolution bindings, so a write that names no node is routed by the
  replica that resolved the patient and refused `400` `target-required` by
  the others. A client sends `openEHR-federation-endpoint` on every write,
  or the operator keeps each client on one replica with balancer affinity.
