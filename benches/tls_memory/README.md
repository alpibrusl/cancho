# Raw output of `scripts/tls_memory.py` (`docs/tls-memory.md`)

What the tables of `docs/tls-memory.md` §4 and §10 were computed from: each line is the server's `VmRSS` (KiB, `/proc/<pid>/status`)
in a phase, as the script printed it, and each `.json` is the same run as `--json` wrote it. The figure in a table is
(RSS in the phase - RSS at `start`) / N.

`x86-64/`: `ssh gram`, Linux 7.0, OpenSSL 3.5.5, run on cores 6 and 7 only (`taskset -c 6,7`) after checking `uptime` and
`docker ps`; RSS only, no timing is taken from these runs.

| file | what |
|---|---|
| `before-tls_echo.log`, `x86_cancho_base.json` | `examples/tls_echo` from `origin/main` (`7bf3628`, with `--connections` up to 1,048,576): the log stops in the OpenSSL run that followed it, which was restarted (`TCP_NODELAY` added to `ossl_hold.c`); the JSON is the complete `tls_echo` run |
| `before-openssl-and-engines.log`, `x86_ossl.json`, `x86_ossl_rel.json`, `x86_engine_base.json` | OpenSSL's server (`benches/server/ossl_hold.c`) default and with `SSL_MODE_RELEASE_BUFFERS`; the engines alone (`tests/programs/tls_pool.cho`) before; then the engines after the first version of the change (superseded by `after.log`) |
| `after-first-version-and-https-before.log`, `x86_https_before.json` | the first version of the change (before review), and `examples/https_hello` before it (to 200 connections: it stops at 1,024, before and after) |
| `after.log`, `x86_cancho_final.json`, `x86_engine_final.json`, `x86_https_final.json` | the change as it is: `tls_echo`, the engines alone, `https_hello` |
| `x86_engine_new64.json` | the engines alone at 10,000 connections on 64 work areas, first version (the areas' clearing and the core's copy do not change what is resident) |

`*/after-rebased.log` and `*_rebased.json` (both machines): the change after its rebase onto `main` with #394 and #396 (`docs/tls-memory.md`
§10.4): `tls_echo`, the engines alone, `https_hello`.

`arm64/`: Docker on the Mac (`lexsys-hooks-env`, native Linux 6.8, OpenSSL 3.0.13), the same runs for that machine:
`before-tls_echo-and-openssl.log` (`tls_echo` before; the OpenSSL run in it was restarted), `before-openssl-and-engines.log`
(OpenSSL default and released, the engines alone before, then after the first version), `after-first-version-and-https-before.log`
(the first version, `https_hello` before, and the engines alone at 100,000 connections on 64 areas, `arm_engine_after100k.json`),
`after.log` (the change as it is). `arm_engine_new64.json`: 10,000 connections on 64 areas, first version.
