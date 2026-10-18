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
# Remote hosts rarely have poopterm's terminfo entry; connect with a common
# TERM instead. Set POOPTERM_SSH_KEEP_TERM=1 to keep xterm-poopterm.
if test "$TERM" = xterm-poopterm; and not set -q POOPTERM_SSH_KEEP_TERM; and not functions -q ssh
	function ssh --wraps ssh
		env TERM=xterm-256color ssh $argv
	end
end
# <<< poopterm shell integration <<<
