#!/usr/bin/env bash
# Wanda LNCM, end to end on this machine: data, distillation on Nebius Token Factory under a hard
# dollar budget, and training on the local NVIDIA GPU.
#
#   training/wanda_lncm/train-local.sh            # everything, resumable
#   STAGES="train" training/wanda_lncm/train-local.sh
#
# The key is read from NEBIUS_TOKEN_FACTORY_KEY, or from a "*nebius*api*.txt" file on the Windows
# desktop. It stays in this process's environment: it is never echoed, written or committed.
#
# Knobs (environment):
#   WANDA_BUDGET_USD   hard spend limit across all distillation steps (default 22)
#   WANDA_RUN          output directory (default ~/wanda-lncm)
#   WANDA_PRESET       small | base (default small: fits an 8 GB card)
#   WANDA_WORLDS       synthetic worlds per language (default 3000)
#   WANDA_MAX_TEMP     pause training at this GPU temperature (default 83)
#   WANDA_RESUME_TEMP  resume at this temperature (default 72)
#   WANDA_GEN_BATCH    generator micro-batch (default: the preset's)
#   WANDA_GEN_ACCUM    generator micro-batches per step (default: the preset's)
#   STAGES             any of: data distill train (default: all three)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN="${WANDA_RUN:-$HOME/wanda-lncm}"
BUDGET="${WANDA_BUDGET_USD:-22}"
PRESET="${WANDA_PRESET:-small}"
WORLDS="${WANDA_WORLDS:-3000}"
MAX_TEMP="${WANDA_MAX_TEMP:-83}"
RESUME_TEMP="${WANDA_RESUME_TEMP:-72}"
STAGES="${STAGES:-data distill train}"
WORKER_RUN="/home/archie/wanda/run"     # the Bonsai2 trainer's GPU-sharing protocol
mkdir -p "$RUN/data" "$RUN/logs"
log() { printf '[%s] %s\n' "$(date +%H:%M:%S)" "$*" | tee -a "$RUN/logs/train-local.log"; }
want() { [[ " $STAGES " == *" $1 "* ]]; }

# ---------------------------------------------------------------- key
find_key_file() {
  local home=""
  if command -v cmd.exe >/dev/null 2>&1; then
    home="$(wslpath "$(cmd.exe /c 'echo %USERPROFILE%' 2>/dev/null | tr -d '\r')" 2>/dev/null || true)"
  fi
  local dirs=()
  [[ -n "$home" ]] && dirs+=("$home/Desktop" "$home/Escritorio" "$home/OneDrive/Desktop" "$home/OneDrive/Escritorio")
  dirs+=("$HOME/Desktop" "$HOME/Escritorio")
  for d in "${dirs[@]}"; do
    [[ -d "$d" ]] || continue
    find "$d" -maxdepth 1 -type f -iname '*nebius*api*.txt' -print -quit 2>/dev/null
  done | head -n 1
}
if want distill && [[ -z "${NEBIUS_TOKEN_FACTORY_KEY:-}" ]]; then
  file="$(find_key_file)"
  [[ -n "$file" ]] || { log "no NEBIUS_TOKEN_FACTORY_KEY and no *nebius*api*.txt on the desktop"; exit 1; }
  # The longest key-shaped token in the file; nothing is printed.
  NEBIUS_TOKEN_FACTORY_KEY="$(grep -oE '[A-Za-z0-9._~+/=-]{20,}' "$file" | awk '{ print length, $0 }' | sort -rn | head -n 1 | cut -d' ' -f2- || true)"
  export NEBIUS_TOKEN_FACTORY_KEY
  log "key read from the desktop file (not shown, not stored)"
fi
if want distill; then
  code="$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $NEBIUS_TOKEN_FACTORY_KEY" https://api.tokenfactory.nebius.com/v1/models || true)"
  [[ "$code" == "200" ]] || { log "Nebius refused the key (HTTP $code)"; exit 1; }
  log "Nebius key accepted"
fi

# ---------------------------------------------------------------- build
FEATURES=""
DEVICE="cpu"
if command -v nvcc >/dev/null 2>&1 && command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi >/dev/null 2>&1; then
  FEATURES="--features cuda"
  DEVICE="cuda:0"
else
  log "no nvcc or no visible GPU: building for CPU. For the GPU install the CUDA toolkit in WSL (Arch: pacman -S cuda; Ubuntu: apt install nvidia-cuda-toolkit) and rerun"
fi
log "building archie-wanda-lncm ($DEVICE)"
(cd "$ROOT/core" && cargo build --release -p archie-wanda-lncm $FEATURES) >>"$RUN/logs/build.log" 2>&1
B="$ROOT/core/target/release/archie-wanda-lncm"

# ---------------------------------------------------------------- data
if want data; then
  if [[ ! -s "$RUN/data/synth/examples.jsonl" ]]; then
    log "generating $WORLDS synthetic worlds per language"
    "$B" synth --out "$RUN/data/synth" --worlds "$WORLDS" --seed 20261020 >>"$RUN/logs/data.log"
  fi
  : > "$RUN/data/tomgen-cases.jsonl.list"
  if compgen -G "$WORKER_RUN/tranches/*.json" >/dev/null; then
    for t in "$WORKER_RUN"/tranches/*.json; do
      out="$RUN/data/tomgen-$(basename "$t" .json).cases.jsonl"
      if "$B" import-tranche --tranche "$t" --out "$out" >>"$RUN/logs/data.log" 2>&1; then
        echo "$out" >> "$RUN/data/tomgen-cases.jsonl.list"
      fi
    done
    log "imported $(wc -l < "$RUN/data/tomgen-cases.jsonl.list") tomgen tranche(s)"
  fi
fi

# ---------------------------------------------------------------- distill
distill_config() {  # $1 = cumulative limit for this step
  local eval_files=""
  if [[ -d /home/archie/wanda/bench/src ]]; then
    while IFS= read -r f; do eval_files+="\"$f\", "; done < <(find /home/archie/wanda/bench/src -type f \( -name '*.json' -o -name '*.jsonl' -o -name '*.csv' \) -size -900M)
  fi
  cat <<EOF
models_dir = "$ROOT/models"
concurrency = 4
timeout_seconds = 240
max_cost_usd = $1
ledger = "$RUN/data/ledger.jsonl"
min_agreement = 0.5
min_safety = 6
trust_gold = true
eval_only = [ ${eval_files} ]
max_eval_overlap = 2

[[teachers]]
model = "nebius.nemotron-3-ultra"
samples = 2
temperature = 0.7
max_tokens = 3072

[[teachers]]
model = "nebius.qwen-3-8-27b"
samples = 1
temperature = 0.7
max_tokens = 3072

[[teachers]]
model = "nebius.gpt-oss-120b"
samples = 1
temperature = 0.7
max_tokens = 3072

[[judges]]
model = "nebius.nemotron-3-super"
max_tokens = 400

[[judges]]
model = "nebius.gpt-oss-120b"
max_tokens = 400

[[drafts]]
model = "nebius.nemotron-3-nano"
max_tokens = 600
EOF
}
if want distill; then
  # Teachers get 70 % of the budget, drafts 5 %, judges the rest; limits are cumulative over
  # the shared ledger, so a rerun continues where it stopped and never exceeds the total.
  teach=$(awk "BEGIN { printf \"%.2f\", $BUDGET * 0.70 }")
  draft=$(awk "BEGIN { printf \"%.2f\", $BUDGET * 0.75 }")
  distill_config "$teach" > "$RUN/data/distill-teach.toml"
  distill_config "$draft" > "$RUN/data/distill-draft.toml"
  distill_config "$BUDGET" > "$RUN/data/distill-judge.toml"
  # Composed answers are where teachers matter; exact labels need none.
  python3 - "$RUN/data/synth/cases.jsonl" "$RUN/data/compose-cases.jsonl" <<'PY'
import json, sys
with open(sys.argv[1]) as src, open(sys.argv[2], "w") as dst:
    for line in src:
        if '"task":"compose"' in line:
            dst.write(line)
PY
  CASES="$RUN/data/compose-cases.jsonl"
  log "teachers (limit \$$teach)"
  "$B" distill teach --config "$RUN/data/distill-teach.toml" --cases "$CASES" --out "$RUN/data/teachers.jsonl" | tee -a "$RUN/logs/distill.log"
  log "drafts (limit \$$draft)"
  "$B" distill draft --config "$RUN/data/distill-draft.toml" --cases "$CASES" --out "$RUN/data/drafts.jsonl" | tee -a "$RUN/logs/distill.log"
  log "judges (limit \$$BUDGET)"
  "$B" distill judge --config "$RUN/data/distill-judge.toml" --cases "$CASES" --samples "$RUN/data/teachers.jsonl" --out "$RUN/data/judges.jsonl" | tee -a "$RUN/logs/distill.log"
  "$B" distill select --config "$RUN/data/distill-judge.toml" --cases "$CASES" \
    --samples "$RUN/data/teachers.jsonl" --samples "$RUN/data/drafts.jsonl" --samples "$RUN/data/judges.jsonl" \
    --out "$RUN/data/distilled.jsonl" | tee -a "$RUN/logs/distill.log"
  spent=$(python3 -c "import json,sys; print(sum(json.loads(l)['cost_micros'] for l in open(sys.argv[1]))/1e6)" "$RUN/data/ledger.jsonl" 2>/dev/null || echo 0)
  log "distillation spent \$$spent of \$$BUDGET (registry prices; Nebius's invoice is authoritative)"
fi

# ---------------------------------------------------------------- train
release_gpu() { rm -f "$WORKER_RUN/hold"; }
if want train; then
  if [[ -d "$WORKER_RUN" ]]; then
    log "asking the Bonsai2 worker to pause (hold file) before using the GPU"
    touch "$WORKER_RUN/hold"
    trap release_gpu EXIT
    for _ in $(seq 1 180); do
      grep -q '"state": *"paused"' "$WORKER_RUN/worker.json" 2>/dev/null && break
      pgrep -f archie_wanda >/dev/null 2>&1 || break
      sleep 10
    done
  fi
  EXAMPLES=(--examples "$RUN/data/synth/examples.jsonl")
  [[ -s "$RUN/data/distilled.jsonl" ]] && EXAMPLES+=(--examples "$RUN/data/distilled.jsonl")
  D="$RUN/ckpt-$PRESET"
  THERMAL=(--max-gpu-temp "$MAX_TEMP" --resume-gpu-temp "$RESUME_TEMP")
  # The generator attends over prompt and every level at once (768 + 340 positions): at 8 rows
  # it needs more than 8 GB, at 4 it peaked at 7.5 GB on an RTX 3070 (2026-10-20). So the small
  # preset keeps its batch of 8 as 4 micro-batches of 2 (4.0 GB peak).
  if [[ "$PRESET" == "small" ]]; then TB=16; GB=2; GA=4; TS=20000; GS=60000; else TB=8; GB=4; GA=1; TS=30000; GS=100000; fi
  GB="${WANDA_GEN_BATCH:-$GB}"; GA="${WANDA_GEN_ACCUM:-$GA}"
  [[ -f "$D/config.toml" ]] || "$B" init --dir "$D" --preset "$PRESET" --vocab 8192 "${EXAMPLES[@]}" | tee -a "$RUN/logs/train.log"
  log "tokenizer ($TS steps, $DEVICE)"
  "$B" train tokenizer --dir "$D" "${EXAMPLES[@]}" --device "$DEVICE" --steps "$TS" --batch "$TB" --lr 3e-4 "${THERMAL[@]}" | tee -a "$RUN/logs/train.log"
  log "generator ($GS steps of $GA x $GB rows, $DEVICE)"
  "$B" train generator --dir "$D" "${EXAMPLES[@]}" --device "$DEVICE" --steps "$GS" --batch "$GB" --accumulate "$GA" --lr 3e-4 \
    --mix gate=0.1,update=0.3,decide=0.3,compose=0.3 "${THERMAL[@]}" | tee -a "$RUN/logs/train.log"
  "$B" calibrate --dir "$D" "${EXAMPLES[@]}" --device "$DEVICE" | tee -a "$RUN/logs/train.log"
  "$B" eval --dir "$D" "${EXAMPLES[@]}" --device "$DEVICE" --limit 500 | tee "$RUN/eval-$PRESET.json"
  log "done: checkpoint $D, evaluation $RUN/eval-$PRESET.json, log $D/train-log.jsonl"
  log "stop cleanly at any time with: touch $D/STOP   (rerun the script to resume)"
fi
