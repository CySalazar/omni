#!/usr/bin/env bash
# =============================================================================
# OMNI OS — deploy bootimage + crea VM VirtualBox sul PC server
# =============================================================================
# Esegui dalla root del repository:
#   bash scripts/deploy-to-server.sh
#
# Prerequisiti locali:
#   - cargo bootimage  (installa con: cargo install bootimage)
#   - sshpass          (installa con: brew install hudochenkov/sshpass/sshpass)
#
# Il server deve avere VirtualBox già installato.
# La VM viene avviata in modalità GUI: compare sul desktop del server
# (visibile via RustDesk o qualunque tool di remote desktop).
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Configurazione
# ---------------------------------------------------------------------------
SERVER="100.118.90.125"
SERVER_USER="matteo"
SERVER_PASS="23890@Barzago"
REMOTE_DIR="/home/matteo/omni-os"
VM_NAME="OMNI-OS-K4"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOOTIMAGE="${REPO_ROOT}/kernel-runner/target/x86_64-unknown-none/debug/bootimage-kernel-runner.bin"

# ---------------------------------------------------------------------------
log()  { echo "  [deploy] $*"; }
ok()   { echo "  [deploy] ✓ $*"; }
fail() { echo "  [deploy] ✗ ERROR: $*" >&2; exit 1; }

echo ""
echo "══════════════════════════════════════════════════════════════"
echo "  OMNI OS — deploy VM su ${SERVER}"
echo "══════════════════════════════════════════════════════════════"
echo ""

# ---------------------------------------------------------------------------
# 0. Controllo prerequisiti locali
# ---------------------------------------------------------------------------
command -v sshpass  >/dev/null 2>&1 || fail "sshpass non trovato.\n  Installa con: brew install hudochenkov/sshpass/sshpass"
command -v bootimage >/dev/null 2>&1 || {
    log "bootimage non trovato — installo..."
    cargo install bootimage
}

# ---------------------------------------------------------------------------
# 1. Build bootimage locale
# ---------------------------------------------------------------------------
log "Build kernel-runner (debug)..."
(cd "${REPO_ROOT}/kernel-runner" && cargo bootimage --target x86_64-unknown-none)

[[ -f "$BOOTIMAGE" ]] || fail "Bootimage non trovato dopo la build: ${BOOTIMAGE}"
ok "Bootimage: $(du -sh "$BOOTIMAGE" | cut -f1)"

# ---------------------------------------------------------------------------
# Helper SSH / SCP
# ---------------------------------------------------------------------------
SSH() {
    sshpass -p "${SERVER_PASS}" ssh \
        -o StrictHostKeyChecking=no \
        -o ConnectTimeout=10 \
        "${SERVER_USER}@${SERVER}" "$@"
}
SCP() {
    sshpass -p "${SERVER_PASS}" scp \
        -o StrictHostKeyChecking=no \
        -o ConnectTimeout=10 \
        "$@"
}

# ---------------------------------------------------------------------------
# 2. Verifica connessione al server
# ---------------------------------------------------------------------------
log "Connessione a ${SERVER}..."
SSH "echo ok" >/dev/null || fail "Impossibile connettersi a ${SERVER_USER}@${SERVER}"
ok "Connessione OK"

# ---------------------------------------------------------------------------
# 3. Trasferimento bootimage
# ---------------------------------------------------------------------------
log "Trasferimento bootimage sul server..."
SSH "mkdir -p ${REMOTE_DIR}"
SCP "${BOOTIMAGE}" "${SERVER_USER}@${SERVER}:${REMOTE_DIR}/bootimage-kernel-runner.bin"
ok "Trasferimento completato"

# ---------------------------------------------------------------------------
# 4. Setup VirtualBox sul server
# ---------------------------------------------------------------------------
log "Configurazione VM '${VM_NAME}' sul server..."

SSH bash << REMOTE
set -euo pipefail
BOOTIMAGE_PATH="${REMOTE_DIR}/bootimage-kernel-runner.bin"
VDI_PATH="${REMOTE_DIR}/omni-os-k4.vdi"
VM="${VM_NAME}"

# Rileva display (RustDesk/X11)
if [[ -z "\${DISPLAY:-}" ]]; then
    # Cerca un display attivo in modo euristico
    for d in :0 :1 :2; do
        if xdpyinfo -display "\$d" &>/dev/null 2>&1; then
            export DISPLAY="\$d"
            break
        fi
    done
    # Fallback: usa :0 anche senza xdpyinfo
    export DISPLAY="\${DISPLAY:-:0}"
fi
echo "  DISPLAY=\${DISPLAY}"

# Converti raw → VDI (sempre, per aggiornare il disco)
echo "  Conversione raw → VDI..."
rm -f "\$VDI_PATH"
VBoxManage convertfromraw "\$BOOTIMAGE_PATH" "\$VDI_PATH" --format VDI
echo "  VDI: \$(du -sh \$VDI_PATH | cut -f1)"

# Crea VM se non esiste
if ! VBoxManage showvminfo "\$VM" &>/dev/null; then
    echo "  Creazione VM '\$VM'..."
    VBoxManage createvm --name "\$VM" --ostype "Linux_64" --register
    VBoxManage modifyvm "\$VM" \
        --memory 128 \
        --cpus 1 \
        --boot1 disk --boot2 none --boot3 none --boot4 none \
        --firmware bios \
        --nic1 none \
        --audio none
    VBoxManage storagectl "\$VM" --name "IDE" --add ide --controller PIIX4
else
    echo "  VM '\$VM' già esistente — aggiorno disco."
    # Stacca il disco precedente e chiudi il media
    VBoxManage storageattach "\$VM" --storagectl "IDE" \
        --port 0 --device 0 --type hdd --medium none 2>/dev/null || true
    VBoxManage closemedium disk "\$VDI_PATH" --delete 2>/dev/null || true
    # Ri-converti (era stato cancellato sopra dal closemedium --delete)
    VBoxManage convertfromraw "\$BOOTIMAGE_PATH" "\$VDI_PATH" --format VDI
fi

# Attacca il disco
VBoxManage storageattach "\$VM" \
    --storagectl "IDE" --port 0 --device 0 \
    --type hdd --medium "\$VDI_PATH"

# Serial port COM1 → file (per ispezione)
VBoxManage modifyvm "\$VM" \
    --uart1 "0x3F8" "4" \
    --uartmode1 "file" "/tmp/omni-os-serial.log"

echo "  Setup VM completato."
echo "SETUP_OK"
REMOTE

# ---------------------------------------------------------------------------
# 5. Avvio VM
# ---------------------------------------------------------------------------
log "Avvio VM '${VM_NAME}' in modalità GUI sul server..."

# Avvio in background (nohup) così lo script ssh può terminare
SSH bash << REMOTE
export DISPLAY="\${DISPLAY:-:0}"
# Prova a rilevare il display se non impostato
if ! xdpyinfo -display "\$DISPLAY" &>/dev/null 2>&1; then
    for d in :0 :1 :2; do
        xdpyinfo -display "\$d" &>/dev/null 2>&1 && export DISPLAY="\$d" && break
    done
fi
nohup VBoxManage startvm "${VM_NAME}" --type gui \
    >/tmp/omni-os-vbox-start.log 2>&1 &
echo "PID=\$!"
REMOTE

echo ""
echo "══════════════════════════════════════════════════════════════"
echo "  ✓  VM '${VM_NAME}' avviata su ${SERVER}"
echo ""
echo "  1. Collegati via RustDesk al server ${SERVER}"
echo "  2. La finestra VirtualBox compare sul desktop del server"
echo "  3. Il banner OMNI OS è visibile per ~10 secondi"
echo "  4. Il kernel si ferma automaticamente"
echo ""
echo "  Log seriale (dopo l'avvio):"
echo "    ssh ${SERVER_USER}@${SERVER} cat /tmp/omni-os-serial.log"
echo ""
echo "  Per riavviare la VM:"
echo "    ssh ${SERVER_USER}@${SERVER} VBoxManage startvm '${VM_NAME}' --type gui"
echo ""
echo "  Per eliminare la VM:"
echo "    ssh ${SERVER_USER}@${SERVER} VBoxManage unregistervm '${VM_NAME}' --delete"
echo "══════════════════════════════════════════════════════════════"
