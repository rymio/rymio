#!/bin/zsh

args=()
skip_next=0

for arg in "$@"; do
  if (( skip_next )); then
    skip_next=0
    if [[ "$arg" == "x86_64-unknown-linux-musl" ]]; then
      continue
    fi
  fi

  case "$arg" in
    --target=x86_64-unknown-linux-musl)
      continue
      ;;
    --target)
      skip_next=1
      continue
      ;;
  esac

  args+=("$arg")
done

exec zig cc -target x86_64-linux-musl "${args[@]}"
