# sha256_builtins.sh -- the script of tests/commands/script_tests.cho for `conformance/sha256_builtins.rs` (deleted): the SHA-256 builtins (docs/sha256-builtin.md section 7) through
# tests/programs/sha256_builtins_driver.cho: FIPS 180-4's one- and two-block examples and a differential against the compression function written out from FIPS 180-4 section 6.2.2, over
# random states (words with bits above the 32nd too, which the builtins ignore) and every block count to 20, now recorded: tests/vectors/sha256_builtins.txt (805 cases) and .expected.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/vector_lib.sh

build tests/programs/sha256_builtins_driver.cho driver

# cpu_has_sha: what this machine's CPU says it has, read the operating system's way: 1, 0, or nothing where it cannot be told (macOS on x86-64).
cpu_has_sha() {
    case $(uname -m) in
        arm64 | aarch64) echo 1 ;; # every Apple CPU, and every aarch64 runner, has FEAT_SHA256
        *)
            if [ -r /proc/cpuinfo ]; then
                if grep '^flags' /proc/cpuinfo | head -1 | tr ' \t' '\n\n' | grep -qx sha_ni; then echo 1; else echo 0; fi
            fi
            ;;
    esac
}

hw=$(answers "$work/driver" H)

begin "the sha256 builtins agree with FIPS 180-4 and the reference"
# On this backend `hw_sha256()` must answer what the CPU says, so a silent fall back to software cannot hide; where it is false the builtins are not exercised (as the Rust test returned).
want_hw=$(cpu_has_sha)
if [ -n "$want_hw" ]; then same "$hw" "$want_hw" "hw_sha256() against the CPU's own answer"; fi
if [ "$hw" = 1 ]; then
    # the two FIPS 180-4 examples, "no blocks" (the state comes back cut to 32 bits), 400 random states with 0 to 20 blocks each as B and one block as W, and one run of 16 KiB.
    same "$(lines tests/vectors/sha256_builtins.txt)" 805 "the cases recorded (a count, so the file cannot shrink silently)"
    agrees "$work/driver" tests/vectors/sha256_builtins.txt tests/vectors/sha256_builtins.expected "the sha256 builtins"
fi
end

begin "a wrong length traps"
# A state that is not 8 words, data that is not whole blocks, or a block that is not 16 words traps, as an index out of bounds does, before any instruction runs; the right sizes do not.
if [ "$hw" = 1 ]; then
    for ok in "b 8 0" "b 8 64" "b 8 640" "w 8 16"; do
        same "$(answers "$work/driver" "$ok")" 0 "$ok"
    done
    for wrong in "b 7 64" "b 9 64" "b 8 63" "b 8 65" "b 8 32" "b 8 96" "b 0 0" "w 7 16" "w 8 15" "w 8 17" "w 24 16"; do
        traps "$work/driver" "$wrong" "a wrong length"
    done
fi
end
