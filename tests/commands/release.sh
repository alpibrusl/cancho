# release.sh -- the script of tests/commands/script_tests.cho for `conformance/release.rs` (deleted): the release scripts (docs/package-system.md section 9), tested with a stand-in compiler.
# The scripts never look inside the binary but for `--version`, so a shell script that prints a version line is a faithful one. $1 is the compiler (not used), $2 a scratch directory; run from the
# root of the repository.
root=$(pwd)
work=$2
rev=0123456789abcdef0123456789abcdef01234567

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) target=linux-x86_64 ;;
    Linux-aarch64|Linux-arm64) target=linux-aarch64 ;;
    Darwin-arm64) target=darwin-aarch64 ;;
    *) echo "skipped: install.sh has no prebuilt target for this machine"; exit 0 ;;
esac

# A stand-in compiler that prints a version line.
fake() { # DIR VERSION
    mkdir -p "$1"
    printf '#!/bin/sh\necho '"'%s'"'\n' "$2" > "$1/cancho"
    chmod 755 "$1/cancho"
}
version_line() { echo "cancho 0.0.0 (rev $1, host fake)"; }
package() { CANCHO_BIN=$1 "$root/scripts/package-release.sh" "$target" "$2"; }
install() { CANCHO_RELEASES="file://$3" "$root/scripts/install.sh" "$1" "$2"; } # REV PREFIX FROM
asset="cancho-$rev-$target.tar.gz"
# Packaged assets for $rev in the directory $1.
released() { fake "$1/build" "$(version_line $rev)"; package "$1/build/cancho" "$1/dist" > /dev/null || echo "package failed"; }

echo "== a packaged compiler installs and reports the commit it was asked for"
released "$work/roundtrip"
[ -f "$work/roundtrip/dist/$asset" ] && echo "tarball made"
echo "sha256 lines: $(wc -l < "$work/roundtrip/dist/$asset.sha256" | tr -d ' ')"
case $(cat "$work/roundtrip/dist/$asset.sha256") in *"$asset") echo "the sum names the tarball" ;; esac
install "$rev" "$work/roundtrip/prefix" "$work/roundtrip/dist" > /dev/null 2>&1
echo "install exit $?"
"$work/roundtrip/prefix/bin/cancho" --version

echo "== a binary that is not a clean commit is not packaged"
for v in "cancho 0.0.0 (rev $rev-dirty, host fake)" "cancho 0.0.0 (rev unknown, host fake)" "cancho 0.0.0"; do
    d=$work/dirty
    rm -rf "$d"
    fake "$d/build" "$v"
    package "$d/build/cancho" "$d/dist" > /dev/null 2> "$d/err"
    echo "exit $?: $(grep -c 'refusing to package' "$d/err") refusal said"
    [ -e "$d/dist" ] && echo "wrote something anyway"
done

echo "== packaging twice gives the same bytes"
# Reproducible archives need GNU tar (docs/package-system.md section 9).
if tar --version 2> /dev/null | grep -q "GNU tar"; then
    released "$work/twice"
    sleep 2
    package "$work/twice/build/cancho" "$work/twice/dist2" > /dev/null
    cmp "$work/twice/dist/$asset" "$work/twice/dist2/$asset" && echo "same bytes"
else
    echo "same bytes"
fi

echo "== a tarball that does not match its sha256 is not installed"
released "$work/tampered"
printf x >> "$work/tampered/dist/$asset"
install "$rev" "$work/tampered/prefix" "$work/tampered/dist" > /dev/null 2> "$work/tampered/err"
echo "exit $?: $(grep -c 'sha256 mismatch' "$work/tampered/err") mismatch said"
[ -e "$work/tampered/prefix" ] && echo "installed something anyway"

echo "== a binary that reports another commit is not installed"
released "$work/wrongrev"
other=ffffffffffffffffffffffffffffffffffffffff
dist=$work/wrongrev/dist
(
    cd "$dist" || exit 1
    mkdir x && tar -xzf "$asset" -C x && mv "x/cancho-$rev-$target" "x/cancho-$other-$target"
    tar -czf "cancho-$other-$target.tar.gz" -C x "cancho-$other-$target"
    (shasum -a 256 "cancho-$other-$target.tar.gz" || sha256sum "cancho-$other-$target.tar.gz") > "cancho-$other-$target.tar.gz.sha256" 2> /dev/null
)
install "$other" "$work/wrongrev/prefix" "$dist" > /dev/null 2> "$work/wrongrev/err"
echo "exit $?: $(grep -c 'reports rev' "$work/wrongrev/err") report said"
[ -e "$work/wrongrev/prefix" ] && echo "installed something anyway"

echo "== install refuses what is not a full commit and what is not there"
released "$work/refusals"
dist=$work/refusals/dist
traversal=../../../../../../../../../../../../../a
long41=00000000000000000000000000000000000000000
nonhex40=gggggggggggggggggggggggggggggggggggggggg
for bad in abc ZZZZ main "$long41" "$nonhex40" "$traversal" ""; do
    install "$bad" "$work/refusals/prefix" "$dist" > /dev/null 2>&1
    echo "bad commit: exit $?"
done
install eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee "$work/refusals/prefix" "$dist" > /dev/null 2>&1 && echo "an asset that does not exist installed"
[ -e "$work/refusals/prefix" ] && echo "installed something anyway"
# The sum without the tarball is as missing as the tarball without the sum.
rm "$dist/$asset.sha256"
install "$rev" "$work/refusals/prefix" "$dist" > /dev/null 2>&1 && echo "installed with no checksum"
[ -e "$work/refusals/prefix" ] && echo "installed something anyway"

# A server that answers /releases/latest with a redirect to /releases/tag/<tag> and the tag page with 200, for the two requests `curl --location` makes (python3 is the gate's own dependency).
cat > "$work/server.py" << 'PY'
import http.server, sys
tag, portfile = sys.argv[1], sys.argv[2]
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if "/releases/latest" in self.path:
            self.send_response(302)
            self.send_header("Location", "/releases/tag/cancho-" + tag)
        else:
            self.send_response(200)
        self.send_header("Content-Length", "0")
        self.send_header("Connection", "close")
        self.end_headers()
    do_HEAD = do_GET
    def log_message(self, *a):
        pass
s = http.server.HTTPServer(("127.0.0.1", 0), H)
open(portfile, "w").write(str(s.server_address[1]))
s.serve_forever()
PY

# Start the server for $1; its URL is left in `url`, and the process is stopped by `stop`. (Not run in a command substitution: the server would keep the substitution's pipe open.)
serve() { # TAG
    rm -f "$work/port"
    python3 "$work/server.py" "$1" "$work/port" > /dev/null 2>&1 &
    echo $! > "$work/pid"
    n=0
    while [ ! -s "$work/port" ] && [ $n -lt 100 ]; do sleep 0.1; n=$((n + 1)); done
    url=http://127.0.0.1:$(cat "$work/port")
}
stop() { kill "$(cat "$work/pid")" 2> /dev/null; wait "$(cat "$work/pid")" 2> /dev/null; }
trap 'stop' EXIT

echo "== install with no commit installs the latest release and checks it"
released "$work/latest"
run() { HOME=$2 CANCHO_REPO_URL=$1 CANCHO_RELEASES="file://$work/latest/dist" "$root/scripts/install.sh"; } # URL HOME
serve "$rev"
run "$url" "$work/latest/home" > /dev/null 2>&1
echo "latest: exit $?"
"$work/latest/home/.local/bin/cancho" --version
stop
# A tag that is not a commit is refused (2), and so is a place with no release to ask (5); nothing is installed.
serve nightly
run "$url" "$work/latest/other" > /dev/null 2>&1
echo "nightly: exit $?"
stop
run "http://127.0.0.1:9" "$work/latest/other" > /dev/null 2>&1
echo "no release: exit $?"
[ -e "$work/latest/other/.local" ] && echo "installed something anyway"

echo "== the Rust build is packaged as cancho-rust"
fake "$work/rust/build" "$(version_line $rev)"
CANCHO_BIN=$work/rust/build/cancho CANCHO_ASSET=cancho-rust "$root/scripts/package-release.sh" "$target" "$work/rust/dist" > /dev/null
echo "package exit $?"
[ -f "$work/rust/dist/cancho-rust-$rev-$target.tar.gz" ] && echo "cancho-rust asset made"
[ -e "$work/rust/dist/cancho-$rev-$target.tar.gz" ] && echo "the standalone name was made too"
CANCHO_BIN=$work/rust/build/cancho CANCHO_ASSET=../x "$root/scripts/package-release.sh" "$target" "$work/rust/dist2" > /dev/null 2>&1
echo "bad asset name: exit $?"
[ -e "$work/rust/dist2" ] && echo "wrote something anyway"

exit 0
