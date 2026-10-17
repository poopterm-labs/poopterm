# >>> poopterm shell integration >>>
# Source this file from ~/.config/fish/config.fish to enable OSC 133 and OSC 7.
if functions -q __poopterm_prompt
	return
end

function __poopterm_prompt --on-event fish_prompt
	set -l status_code $status
	printf '\033]133;D;%s\007\033]133;A\007' $status_code
	set -l encoded (string escape --style=url -- $PWD | string replace -a '%2F' '/')
	printf '\033]7;file://%s\007' $encoded
end

function __poopterm_preexec --on-event fish_preexec
	printf '\033]133;C\007'
end
# <<< poopterm shell integration <<<
