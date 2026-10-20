# >>> poopterm shell integration >>>
# Source this file from ~/.bashrc to enable OSC 133 and OSC 7 integration.
if [[ ${__POOPTERM_INTEGRATION_LOADED:-} == 1 ]]; then
	return
fi
__POOPTERM_INTEGRATION_LOADED=1

__poopterm_original_prompt_command=${PROMPT_COMMAND-}
__poopterm_original_ps0=${PS0-}

# Percent-encodes every byte of $1 outside the URI-unreserved set into
# REPLY, so control characters cannot end an OSC sequence early.
__poopterm_url_encode() {
	local LC_ALL=C text=$1 char code i
	REPLY=
	for (( i = 0; i < ${#text}; i++ )); do
		char=${text:i:1}
		case $char in
			[a-zA-Z0-9/._~-]) REPLY+=$char ;;
			# bash 3.2 (macOS) sign-extends bytes >= 0x80; keep one byte.
			*) printf -v code '%d' "'$char"; printf -v char '%%%02X' $(( code & 255 )); REPLY+=$char ;;
		esac
	done
}

# Runs from PS0 (bash 4.4+), once a command is read: reports it with OSC 133
# C. History is the only place bash keeps it, and its newest entry is this
# command only if history grew since the prompt; ignorespace, ignoredups and
# `set +o history` add none.
__poopterm_preexec() {
	local next='\!' entry REPLY
	[[ ${next@P} != "${__poopterm_history-}" ]] && entry=$(HISTTIMEFORMAT= builtin history 1)
	if [[ -z $entry ]]; then
		printf '\033]133;C\007'
		return
	fi
	# "   42  command", or "   42* command" once edited.
	entry=${entry#"${entry%%[0-9]*}"}
	entry=${entry#"${entry%%[!0-9]*}"}
	__poopterm_url_encode "${entry:2}"
	printf '\033]133;C;cmdline_url=%s\007' "$REPLY"
}

__poopterm_prompt_command() {
	local status=$? next='\!' REPLY
	printf '\033]133;D;%s\007' "$status"
	# The history number the next command gets, for __poopterm_preexec.
	if (( BASH_VERSINFO[0] * 100 + BASH_VERSINFO[1] >= 404 )); then
		__poopterm_history=${next@P}
	fi
	if [[ -n $__poopterm_original_prompt_command ]]; then
		eval "$__poopterm_original_prompt_command"
	fi
	# Clicks in the input line may move the cursor with arrow keys.
	__poopterm_url_encode "$PWD"
	# Naming the host keeps ssh sessions' directories apart from local ones.
	printf ']133;A;cl=line]7;file://%s%s' "$HOSTNAME" "$REPLY"
}

# Remote hosts rarely have poopterm's terminfo entry; connect with a common
# TERM instead. Set POOPTERM_SSH_KEEP_TERM=1 to keep xterm-poopterm.
if [[ $TERM == xterm-poopterm && -z ${POOPTERM_SSH_KEEP_TERM-} ]] && ! declare -F ssh >/dev/null; then
	ssh() { TERM=xterm-256color command ssh "$@"; }
fi

PROMPT_COMMAND=__poopterm_prompt_command
# B marks where command input starts, so it belongs at the end of the prompt.
PS1="$PS1"'\[\e]133;B\a\]'
PS0='$(__poopterm_preexec)'"$__poopterm_original_ps0"
# <<< poopterm shell integration <<<
