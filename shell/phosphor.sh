#!/usr/bin/env bash
# phosphor shell integration
# Source this file in your .bashrc, or preferably use: eval "$(phosphor init bash)"

# Guard: only run in interactive shells
[[ $- != *i* ]] && return

# Guard: don't activate inside nested shells if already active
[[ -n "$PHOSPHOR_ACTIVE" ]] && return

# Read timeout from config, default 300 seconds
__phosphor_timeout() {
    local timeout
    timeout=$(phosphor print-timeout 2>/dev/null)
    echo "${timeout:-300}"
}

# Set TMOUT to trigger ALRM after idle period
TMOUT=$(__phosphor_timeout)

# Trap ALRM signal -- bash sends this when TMOUT expires at prompt
__phosphor_activate() {
    # Don't activate if stdin isn't a terminal
    [[ ! -t 0 ]] && return

    # Don't activate if another instance is running
    [[ -n "$PHOSPHOR_ACTIVE" ]] && return

    export PHOSPHOR_ACTIVE=1
    phosphor run
    unset PHOSPHOR_ACTIVE
}

trap '__phosphor_activate' ALRM
