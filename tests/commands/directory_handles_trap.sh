# directory_handles_trap.sh -- the second script of tests/commands/directory_handles.cho. $1 is the probe (tests/programs/directory_handles_prefix_probe.cho, built), $2 a scratch directory.
# `open_dir` of `/etc` under a capability narrowed to `/tmp/cancho-dirs-prefix` must kill the program, as `open_read` outside its prefix does: a shell reports a program killed by a signal as 128 and more.
"$1" > /dev/null 2>&1
status=$?
if [ "$status" -gt 128 ]; then echo "killed by a signal"; else echo "exited $status"; fi
