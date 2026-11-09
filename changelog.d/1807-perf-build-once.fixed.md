- The performance gate builds the base and the head once, in a job of its
  own, and hands them to each attempt as an artifact, so a slow build no
  longer uses up an attempt's time limit. An attempt cancelled while the run
  goes on (a runner that shut down, or a job that reached its time limit) is
  followed by the next attempt instead of failing the gate. No threshold
  changes (#1807).
