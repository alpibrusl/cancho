# Raw output of `docs/record-in-place.md` §7

| file | what |
|---|---|
| `kept-x86-64.txt` | §7.1: cycles and instructions a kept `https_hello` request, before, after and OpenSSL's `oserv`, three rounds |
| `kept-steps-x86-64.txt` | §7.1: the steps on the way, and the profiles' shares |
| `profile-base-x86-64.txt`, `profile-new-x86-64.txt` | §7.1: `perf report --children` of the server, the engine's functions |
| `cost-x86-64.txt` | §7.1: `https_hello_test.py --cost` (server CPU time), disturbed by the clock, not used |
| `engine-cost-x86-64.txt`, `engine-cost-macos-m4.txt` | §7.2: `tests/programs/engine_cost.cho` |
| `record-cost-x86-64.txt` | §7.2: `record_cost.cho`, `gcm_cost.cho` and `openssl speed`, the clock moving between runs |
| `big-x86-64.txt` | §7.3: `/big/16777216` server cycles a byte |
| `memory-*-x86-64.txt` | §7.4: `scripts/tls_memory.py`, before and after |
| `differentials-*.txt` | §7.5: the three differentials on each machine and backend |
| `mutants-*.txt` | §7.7: the mutant runs |
| `timing-*.txt` | §7.8: dudect, in place, 10^6 measurements a test |
