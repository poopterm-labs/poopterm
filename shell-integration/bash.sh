# >>> poopterm shell integration >>>
# Source this file from ~/.bashrc to enable OSC 133 and OSC 7 integration.
if [[ ${__POOPTERM_INTEGRATION_LOADED:-} == 1 ]]; then
	return
fi
__POOPTERM_INTEGRATION_LOADED=1

__poopterm_original_prompt_command=${PROMPT_COMMAND-}
__poopterm_original_ps0=${PS0-}

__poopterm_escape_path() {
	local path=$PWD
	path=${path//%/%25}
	path=${path// /%20}
	path=${path//#/%23}
	path=${path//\?/%3F}
	printf '%s' "$path"
}

__poopterm_prompt_command() {
	local status=$?
	printf '\033]133;D;%s\007' "$status"
	if [[ -n $__poopterm_original_prompt_command ]]; then
		eval "$__poopterm_original_prompt_command"
	fi
	printf '\033]133;A\007\033]7;file://%s\007' "$( __poopterm_escape_path )"
}

PROMPT_COMMAND=__poopterm_prompt_command
# B marks where command input starts, so it belongs at the end of the prompt.
PS1="$PS1"'\[\e]133;B\a\]'
PS0=$'\033]133;C\007'"$__poopterm_original_ps0"
# <<< poopterm shell integration <<<
