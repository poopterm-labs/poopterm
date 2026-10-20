# >>> poopterm shell integration >>>
# Source this file from ~/.config/fish/config.fish to enable OSC 133 and OSC 7.
if functions -q __poopterm_prompt
	return
end

# fish 4 marks prompts and commands itself (its mark-prompt feature) and
# asks for click events; marking them again would close each command twice.
function __poopterm_prompt --on-event fish_prompt
	set -l status_code $status
	if not status test-feature mark-prompt 2>/dev/null
		# Clicks in the input line may move the cursor with arrow keys.
		printf '\033]133;D;%s\007\033]133;A;cl=line\007' $status_code
		__poopterm_mark_input
	end
	set -l encoded (string escape --style=url -- $PWD | string replace -a '%2F' '/')
	# Naming the host keeps ssh sessions' directories apart from local ones.
	printf '\033]7;file://%s%s\007' $hostname $encoded
end

# Ends the prompt with the start of command input (B), wrapping the prompt
# function again whenever it is replaced, as themes do.
function __poopterm_mark_input
	if functions fish_prompt 2>/dev/null | string match -q '*__poopterm_user_prompt*'
		return
	end
	functions -e __poopterm_user_prompt
	functions -c fish_prompt __poopterm_user_prompt 2>/dev/null; or return
	function fish_prompt
		__poopterm_user_prompt
		printf '\033]133;B\007'
	end
end

if not status test-feature mark-prompt 2>/dev/null
	function __poopterm_preexec --on-event fish_preexec
		printf '\033]133;C;cmdline_url=%s\007' (string escape --style=url -- $argv[1])
	end
end
# Remote hosts rarely have poopterm's terminfo entry; connect with a common
# TERM instead. Set POOPTERM_SSH_KEEP_TERM=1 to keep xterm-poopterm.
if test "$TERM" = xterm-poopterm; and not set -q POOPTERM_SSH_KEEP_TERM; and not functions -q ssh
	function ssh --wraps ssh
		env TERM=xterm-256color ssh $argv
	end
end
# <<< poopterm shell integration <<<
