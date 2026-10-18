# poopterm: loaded from XDG_DATA_DIRS. Removes poopterm's entry from
# XDG_DATA_DIRS so child programs see your original value.
if set -q POOPTERM_FISH_DATA_DIR
	set -gx XDG_DATA_DIRS (string replace -- "$POOPTERM_FISH_DATA_DIR:" "" "$XDG_DATA_DIRS")
	set -e POOPTERM_FISH_DATA_DIR
end
