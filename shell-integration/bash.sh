# >>> poopterm shell integration >>>
# Source this file from ~/.bashrc to enable OSC 133 and OSC 7 integration.
if [[ ${__POOPTERM_INTEGRATION_LOADED:-} == 1 ]]; then
	return
fi
__POOPTERM_INTEGRATION_LOADED=1

__poopterm_original_prompt_command=${PROMPT_COMMAND-}
__poopterm_original_ps0=${PS0-}

# Percent-encodes every byte of $PWD outside the URI-unreserved set, so
# control characters in directory names cannot end the OSC 7 sequence.
__poopterm_escape_path() {
	local LC_ALL=C path=$PWD encoded= char code i
	for (( i = 0; i < ${#path}; i++ )); do
		char=${path:i:1}
		case $char in
			[a-zA-Z0-9/._~-]) encoded+=$char ;;
			# bash 3.2 (macOS) sign-extends bytes >= 0x80; keep one byte.
			*) printf -v code '%d' "'$char"; printf -v char '%%%02X' $(( code & 255 )); encoded+=$char ;;
		esac
	done
	printf '%s' "$encoded"
}

__poopterm_prompt_command() {
	local status=$?
	printf '\033]133;D;%s\007' "$status"
	if [[ -n $__poopterm_original_prompt_command ]]; then
		eval "$__poopterm_original_prompt_command"
	fi
	# Clicks in the input line may move the cursor with arrow keys.
	printf '\033]133;A;cl=line\007\033]7;file://%s\007' "$( __poopterm_escape_path )"
}

# Remote hosts rarely have poopterm's terminfo entry; connect with a common
# TERM instead. Set POOPTERM_SSH_KEEP_TERM=1 to keep xterm-poopterm.
if [[ $TERM == xterm-poopterm && -z ${POOPTERM_SSH_KEEP_TERM-} ]] && ! declare -F ssh >/dev/null; then
	ssh() { TERM=xterm-256color command ssh "$@"; }
fi

PROMPT_COMMAND=__poopterm_prompt_command
# B marks where command input starts, so it belongs at the end of the prompt.
PS1="$PS1"'\[\e]133;B\a\]'
PS0=$'\033]133;C\007'"$__poopterm_original_ps0"
# <<< poopterm shell integration <<<
