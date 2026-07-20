#!/usr/bin/env bash
# Aixeca un sshd propi al port 2222 (sense root, sense contenidors) i ofereix un
# menú de proves. El servidor es queda viu entre proves. Ús: ./test.sh
set -euo pipefail

cd "$(dirname "$0")"
ENV_DIR="/tmp/sftp-rust-test"   # fora del repo: el mount compartit ignora chmod i sshd exigeix 0600
PORT=2222
BIN="$PWD/target/debug/sftp-rust"
SRC="$ENV_DIR/src"
DEST="$ENV_DIR/dest"
BACK="$ENV_DIR/back"
REMOTE="$USER@127.0.0.1"

cleanup() { [[ -n "${SSHD_PID:-}" ]] && kill "$SSHD_PID" 2>/dev/null; }
trap cleanup EXIT

# --- entorn: clau, host key, config ---
rm -rf "$ENV_DIR"
mkdir -p "$ENV_DIR"
ssh-keygen -q -t ed25519 -N '' -f "$ENV_DIR/id_test"
ssh-keygen -q -t ed25519 -N '' -f "$ENV_DIR/hostkey"
cp "$ENV_DIR/id_test.pub" "$ENV_DIR/authorized_keys"
chmod 600 "$ENV_DIR/authorized_keys"

cat > "$ENV_DIR/sshd_config" <<EOF
Port $PORT
ListenAddress 127.0.0.1
HostKey $ENV_DIR/hostkey
AuthorizedKeysFile $ENV_DIR/authorized_keys
PidFile $ENV_DIR/sshd.pid
Subsystem sftp /usr/lib/ssh/sftp-server
StrictModes no
PasswordAuthentication no
UsePAM no
EOF

/usr/bin/sshd -f "$ENV_DIR/sshd_config" -E "$ENV_DIR/sshd.log"
SSHD_PID=$(cat "$ENV_DIR/sshd.pid")

cargo build || exit 1
[[ -x "$BIN" ]] || { echo "no s'ha compilat"; exit 1; }

# --- arbre de prova, refet abans de cada cas ---
reset_env() {
  rm -rf "$SRC" "$DEST" "$BACK"
  mkdir -p "$SRC/sub/deep" "$DEST" "$BACK"
  echo "hola" > "$SRC/petit.txt"
  head -c 3M /dev/urandom > "$SRC/gran.bin"   # prova el streaming per trossos
  echo "niu" > "$SRC/sub/deep/fons.txt"
}

run() {
  echo "  \$ sftp-rust -i <clau> -P $PORT $*"
  # sense set -e: si falla volem tornar al menú, no morir
  "$BIN" -i "$ENV_DIR/id_test" -P $PORT "$@" && return 0 || { echo "  ^ ha retornat $?"; return 1; }
}
check() { diff -r "$1" "$2" >/dev/null && echo "  OK   $1 == $2" || { echo "  FALLA  $1 != $2"; return 1; }; }

t_push_file()  { reset_env; run "$SRC/petit.txt" "$REMOTE:$DEST/petit.txt" && check "$SRC/petit.txt" "$DEST/petit.txt"; }
t_push_big()   { reset_env; run "$SRC/gran.bin"  "$REMOTE:$DEST/gran.bin"  && check "$SRC/gran.bin"  "$DEST/gran.bin"; }
t_push_rec()   { reset_env; run -r -p "$SRC" "$REMOTE:$DEST" && check "$SRC" "$DEST/src"; }
t_pull_file()  { reset_env; run "$REMOTE:$SRC/petit.txt" "$BACK/petit.txt" && check "$SRC/petit.txt" "$BACK/petit.txt"; }
t_pull_rec()   { reset_env; run -r -p "$REMOTE:$SRC" "$BACK" && check "$SRC" "$BACK/src"; }
t_all()        { for t in t_push_file t_push_big t_push_rec t_pull_file t_pull_rec; do echo "--- $t"; $t || true; done; }
t_shell()      { echo "  sessió sftp manual (Ctrl-D per sortir)"; sftp -i "$ENV_DIR/id_test" -P $PORT -o StrictHostKeyChecking=no "$REMOTE"; }

echo "sshd viu al port $PORT (pid $SSHD_PID) · entorn a $ENV_DIR"
PS3=$'\nQuè vols provar? '
select opt in \
  "push: fitxer" "push: fitxer gran (3M)" "push: recursiu -r -p" \
  "pull: fitxer" "pull: recursiu -r -p" \
  "tot" "sftp manual (referència)" "sortir"; do
  case "$REPLY" in
    1) t_push_file || true ;;
    2) t_push_big  || true ;;
    3) t_push_rec  || true ;;
    4) t_pull_file || true ;;
    5) t_pull_rec  || true ;;
    6) t_all ;;
    7) t_shell || true ;;
    8) break ;;
    *) echo "opció no vàlida" ;;
  esac
done
