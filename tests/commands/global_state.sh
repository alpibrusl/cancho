# global_state.sh -- the script of tests/commands/script_tests.cho that was the last test of crates/cancho/tests/conformance/global_state.rs (docs/gate.md G4, wave 4; the rest of that file moved to global_state.cho in wave 2).
# docs/unsafe-and-global-state.md (#587) section 5: state that outlives the thing it described must not be usable by the next one. The test needs a server that accepts connections and holds them while the program
# runs: tests/programs/sk_holder.cho, started as the other socket scripts start it (tests/support/sk.sh). One backend (LLVM): the Cranelift one is retired with the Rust compiler (D2). $1 is the compiler, $2 a scratch
# directory; run from the root of the repository.
. tests/support/sk.sh
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_lib.sh

sk_build sk_holder tests/programs/sk_holder.cho

# `lexs_fd_epoch`. A ticket from `conn_detach` names a descriptor and the epoch it had; the table is the one piece of state a long-lived server carries from one connection to the next. When a connection ends and another takes its
# descriptor number, the first one's ticket must not reach the second.
begin a_ticket_does_not_reach_the_connection_that_took_its_descriptor_number
sk_start "$SK_SCRATCH/sk_holder"
sed -e "s/BOUND/127.0.0.1:$port/g" -e "s/PORT/$port/g" tests/programs/global_state_fd_reuse.tmpl > "$work/reuse.cho"
r build "$work/reuse.cho" --std -o "$work/reuse"; succeeds "the program builds"
"$work/reuse" > /dev/null 2>&1
c=$?
sk_stop
same "$c" 0 "the score (101 means the number was not reused, so nothing was shown; the status minus 100 is the score)"
end
