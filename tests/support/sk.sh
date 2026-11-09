# sk.sh -- the shell half of the scripted socket client (docs/gate.md, G4 wave 3), sourced by the scripts of tests/commands/ that drive a server:
#     . tests/support/sk.sh
# $1 is the compiler and $2 a scratch directory, as for every script of tests/commands/script_tests.cho; run from the root of the repository.
# The client is tests/programs/sk_client.cho (a cancho program: see its header for the commands of a script), built once by `sk_setup`.
#   sk_setup                 build the client into $2/sk_client
#   sk_build NAME FILES...   build a program into $2/NAME with the compiler $1
#   sk_start EXE ARGS...     start EXE on a free port (the first argument), in the background: $port and $pid; a server that is gone within
#                            0.4 seconds found its port taken, and is started again on another (five times)
#   sk_stop                  kill the server
#   sk_client SCRIPT         run a script of the client against $port
SK_CANCHO=$1
SK_SCRATCH=$2
sk_build() { # NAME FILES...
    sk_name=$1; shift
    "$SK_CANCHO" build "$@" --std -o "$SK_SCRATCH/$sk_name" 2>&1 || { echo "build of $sk_name failed"; exit 1; }
}
sk_setup() {
    sk_build sk_client tests/programs/sk_client.cho
}
sk_start() { # EXE ARGS...
    sk_exe=$1; shift
    sk_try=0
    while [ $sk_try -lt 5 ]; do
        port=$(awk 'BEGIN { srand(); print 30000 + int(rand() * 20000) }')
        "$sk_exe" "$port" "$@" >/dev/null 2>&1 &
        pid=$!
        sleep 0.4
        kill -0 $pid 2>/dev/null && return 0
        sk_try=$((sk_try + 1))
    done
    echo "the server never stayed up"
    exit 1
}
sk_stop() {
    kill -9 $pid 2>/dev/null
    wait $pid 2>/dev/null
    return 0
}
sk_client() { # SCRIPT
    "$SK_SCRATCH/sk_client" "$port" "$1"
}
