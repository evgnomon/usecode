#!/usr/bin/env bash
# =============================================================================
# COMPREHENSIVE BASH FEATURES REFERENCE SCRIPT
# =============================================================================
# Author:  Claude (Anthropic)
# Created: 2026-03-28
# Description: Demonstrates virtually every Bash feature in one script.
# Usage: bash bash_features.sh [args...]
# =============================================================================

set -uo pipefail  # Strict mode: unset vars, pipe failures
# Note: 'set -e' (errexit) is demonstrated but not used globally,
# as many demo sections intentionally trigger non-zero exits.
IFS=$'\n\t'        # Safer word splitting

# =============================================================================
# 1. SHEBANG & COMMENTS
# =============================================================================
# Single-line comment
: 'This is a multi-line comment
   using the colon builtin with a single-quoted string.
   Bash ignores the argument to ":".'


# =============================================================================
# 2. VARIABLES & DATA TYPES
# =============================================================================

# --- 2.1 Simple Variables ---
name="Bash"
version=5
greeting="Hello from $name version $version!"
echo "$greeting"

# --- 2.2 Readonly / Constants ---
readonly PI=3.14159
declare -r AUTHOR="Claude"
echo "PI=$PI, Author=$AUTHOR"

# --- 2.3 Local Variables (inside functions) ---
my_func() {
    local local_var="I'm local"
    echo "$local_var"
}
my_func

# --- 2.4 Integer Variables ---
declare -i counter=0
counter+=5        # Arithmetic assignment (becomes 5, not "05")
echo "Counter: $counter"

# --- 2.5 Uppercase / Lowercase Attributes ---
declare -u shout="whisper"   # Auto-uppercase
declare -l quiet="YELLING"   # Auto-lowercase
echo "shout=$shout, quiet=$quiet"

# --- 2.6 Nameref Variables (Bash 4.3+) ---
original="I am the original"
declare -n ref=original
echo "Nameref: $ref"
ref="Modified via ref"
echo "Original after nameref change: $original"

# --- 2.7 Environment / Export ---
export MY_ENV_VAR="exported"
env_child() { echo "From child: $MY_ENV_VAR"; }
env_child


# =============================================================================
# 3. QUOTING
# =============================================================================

echo '--- Quoting ---'
echo 'Single quotes: no $expansion, no \escapes (except '\''this trick'\'')'
echo "Double quotes: expansion works — name=$name"
echo "Escape sequences: tab\there, newline below"
echo $'ANSI-C quoting: tab\there, newline\nhere, bell\a'
echo "Command substitution: $(date +%Y)"
echo "Arithmetic: $((2 ** 10))"


# =============================================================================
# 4. ARRAYS
# =============================================================================

echo '--- Indexed Arrays ---'

# --- 4.1 Indexed Arrays ---
fruits=("apple" "banana" "cherry" "date")
fruits+=("elderberry")              # Append
fruits[10]="fig"                    # Sparse index

echo "First: ${fruits[0]}"
echo "All:   ${fruits[@]}"
echo "Count: ${#fruits[@]}"
echo "Indices: ${!fruits[@]}"
echo "Slice [1..3]: ${fruits[@]:1:3}"

# Iterate
for fruit in "${fruits[@]}"; do
    echo "  fruit: $fruit"
done

# --- 4.2 Associative Arrays (Bash 4+) ---
echo '--- Associative Arrays ---'
declare -A colors
colors=([red]="#FF0000" [green]="#00FF00" [blue]="#0000FF")
colors[white]="#FFFFFF"

echo "Red: ${colors[red]}"
echo "Keys: ${!colors[@]}"
echo "Values: ${colors[@]}"
echo "Size: ${#colors[@]}"

for key in "${!colors[@]}"; do
    echo "  $key => ${colors[$key]}"
done

# --- 4.3 Array Operations ---
unset 'fruits[1]'                   # Delete element
echo "After unset [1]: ${fruits[@]}"

# Copying an array
copy=("${fruits[@]}")

# Merging arrays
merged=("${fruits[@]}" "${copy[@]}")
echo "Merged count: ${#merged[@]}"

# Readarray / mapfile
mapfile -t lines <<< $'line1\nline2\nline3'
echo "mapfile lines: ${lines[@]}"


# =============================================================================
# 5. STRING OPERATIONS
# =============================================================================

echo '--- String Operations ---'
str="Hello, World! Hello, Bash!"

# Length
echo "Length: ${#str}"

# Substring extraction
echo "Substring [7..12]: ${str:7:5}"

# Substring removal
echo "Remove shortest prefix *o:  ${str#*o}"
echo "Remove longest prefix *o:   ${str##*o}"
echo "Remove shortest suffix o*:  ${str%o*}"
echo "Remove longest suffix o*:   ${str%%o*}"

# Search and replace
echo "Replace first Hello:  ${str/Hello/Hi}"
echo "Replace all Hello:    ${str//Hello/Hi}"
echo "Replace prefix Hello: ${str/#Hello/Hi}"
echo "Replace suffix Bash!: ${str/%Bash!/Zsh!}"

# Case modification (Bash 4+)
word="hElLo"
echo "Uppercase first: ${word^}"
echo "Uppercase all:   ${word^^}"
echo "Lowercase first: ${word,}"
echo "Lowercase all:   ${word,,}"
echo "Toggle first:    ${word~}"
echo "Toggle all:      ${word~~}"

# Default values
unset maybe
echo "Default if unset:    ${maybe:-default_value}"
echo "Assign if unset:     ${maybe:=assigned_value}"
echo "Now maybe=$maybe"
echo "Error if unset:      ${maybe:+alternative}"   # alternative since set
# ${maybe:?error message} would exit if unset

# Indirect expansion
var_name="name"
echo "Indirect: ${!var_name}"    # Expands $name

# Regex match extract (Bash 3.2+)
if [[ "file_2026.txt" =~ ([0-9]{4}) ]]; then
    echo "Regex capture group: ${BASH_REMATCH[1]}"
fi


# =============================================================================
# 6. ARITHMETIC
# =============================================================================

echo '--- Arithmetic ---'

# --- 6.1 Arithmetic Expansion ---
echo "Add: $((3 + 5))"
echo "Multiply: $((4 * 7))"
echo "Power: $((2 ** 16))"
echo "Modulo: $((17 % 5))"
echo "Ternary: $(( 10 > 5 ? 1 : 0 ))"
echo "Bitwise AND: $(( 0xFF & 0x0F ))"
echo "Bitwise OR: $(( 0xF0 | 0x0F ))"
echo "Bitwise XOR: $(( 0xFF ^ 0x0F ))"
echo "Bitwise shift: $(( 1 << 8 ))"
echo "Comma operator: $(( x=5, y=10, x+y ))"

# --- 6.2 let ---
let "a = 5 + 3"
let "a += 2"
echo "let result: $a"

# --- 6.3 (( )) compound command ---
(( b = a * 2 ))
echo "(( )) result: $b"

# --- 6.4 Increment / Decrement ---
(( b++ ))
(( b-- ))
(( ++b ))
(( --b ))
echo "After inc/dec: $b"

# --- 6.5 Different Bases ---
echo "Octal 077:  $(( 077 ))"       # 63
echo "Hex 0xFF:   $(( 0xFF ))"      # 255
echo "Binary 2#1010: $(( 2#1010 ))" # 10
echo "Base36 36#z: $(( 36#z ))"     # 35


# =============================================================================
# 7. CONDITIONALS
# =============================================================================

echo '--- Conditionals ---'

# --- 7.1 if / elif / else ---
value=42
if (( value > 100 )); then
    echo "Big"
elif (( value > 10 )); then
    echo "Medium"
else
    echo "Small"
fi

# --- 7.2 test / [ ] ---
if [ -n "$name" ]; then
    echo "name is non-empty"
fi

if [ "$name" = "Bash" ]; then
    echo "name equals Bash"
fi

# --- 7.3 [[ ]] (extended test) ---
if [[ "$name" == B* && ${#name} -gt 2 ]]; then
    echo "Pattern + compound test passed"
fi

if [[ "hello123" =~ ^[a-z]+[0-9]+$ ]]; then
    echo "Regex match in [[ ]]"
fi

# --- 7.4 case statement ---
os="Linux"
case "$os" in
    Linux|GNU)
        echo "Case: Linux-like"
        ;;&                         # Fall-through to test next pattern (Bash 4+)
    *inux*)
        echo "Case: Contains 'inux'"
        ;;
    macOS|Darwin)
        echo "Case: macOS"
        ;;
    *)
        echo "Case: Unknown OS"
        ;;
esac

# --- 7.5 select (menu) ---
# Uncomment to test interactively:
# select opt in "Option A" "Option B" "Quit"; do
#     case $opt in
#         "Quit") break ;;
#         *) echo "You chose: $opt" ;;
#     esac
# done

# --- 7.6 Short-circuit / Ternary-style ---
[[ -d /tmp ]] && echo "/tmp exists" || echo "/tmp missing"


# =============================================================================
# 8. LOOPS
# =============================================================================

echo '--- Loops ---'

# --- 8.1 for loop (list) ---
for item in one two three; do
    echo "  for-in: $item"
done

# --- 8.2 for loop (C-style) ---
for (( i=0; i<3; i++ )); do
    echo "  C-style: $i"
done

# --- 8.3 while loop ---
n=3
while (( n > 0 )); do
    echo "  while: $n"
    (( n-- ))
done

# --- 8.4 until loop ---
n=0
until (( n >= 3 )); do
    echo "  until: $n"
    (( ++n ))
done

# --- 8.5 Loop over array ---
for f in "${fruits[@]}"; do echo "  array loop: $f"; done

# --- 8.6 Loop over associative array ---
for k in "${!colors[@]}"; do echo "  assoc: $k=${colors[$k]}"; done

# --- 8.7 Loop with glob ---
for f in /etc/host*; do
    [[ -e "$f" ]] && echo "  glob: $f"
done

# --- 8.8 while read (line-by-line) ---
echo -e "alpha\nbeta\ngamma" | while IFS= read -r line; do
    echo "  read: $line"
done

# --- 8.9 break and continue ---
for i in {1..10}; do
    (( i == 3 )) && continue
    (( i == 7 )) && break
    echo "  break/continue: $i"
done

# --- 8.10 Infinite loop ---
count=0
while true; do
    (( ++count >= 3 )) && break
done
echo "  Infinite loop ran $count times"


# =============================================================================
# 9. FUNCTIONS
# =============================================================================

echo '--- Functions ---'

# --- 9.1 Basic function ---
greet() {
    echo "Hi, ${1:-stranger}!"
}
greet "World"
greet

# --- 9.2 Function with return value ---
is_even() {
    (( $1 % 2 == 0 ))   # Exit status: 0 = true, 1 = false
}
if is_even 4; then echo "4 is even"; fi

# --- 9.3 Function returning string via stdout ---
get_hostname() {
    echo "$(hostname 2>/dev/null || echo "unknown")"
}
host="$(get_hostname)"
echo "Host: $host"

# --- 9.4 Function with local and nameref ---
swap() {
    local -n __a=$1 __b=$2
    local tmp="$__a"
    __a="$__b"
    __b="$tmp"
}
x="first" y="second"
swap x y
echo "After swap: x=$x, y=$y"

# --- 9.5 Recursive function ---
factorial() {
    local n=$1
    if (( n <= 1 )); then
        echo 1
    else
        echo $(( n * $(factorial $(( n - 1 ))) ))
    fi
}
echo "5! = $(factorial 5)"

# --- 9.6 Function with variable arguments ---
join_strings() {
    local sep="$1"; shift
    local result="$1"; shift
    for s in "$@"; do
        result+="${sep}${s}"
    done
    echo "$result"
}
echo "Joined: $(join_strings ", " "a" "b" "c" "d")"

# --- 9.7 Function scope demonstration ---
outer_var="outer"
scope_demo() {
    local outer_var="shadowed"
    echo "Inside: $outer_var"
}
scope_demo
echo "Outside: $outer_var"


# =============================================================================
# 10. INPUT / OUTPUT & REDIRECTION
# =============================================================================

echo '--- I/O & Redirection ---'

# --- 10.1 Standard redirections ---
echo "stdout to file" > /tmp/bash_demo_out.txt
echo "append to file" >> /tmp/bash_demo_out.txt
cat /tmp/bash_demo_out.txt

# --- 10.2 stderr redirection ---
ls /nonexistent_path 2>/dev/null         # Suppress errors
ls /nonexistent_path 2>&1 || true        # Merge stderr into stdout

# --- 10.3 stdin redirection ---
wc -l < /tmp/bash_demo_out.txt

# --- 10.4 Here Document ---
cat <<EOF
  Here document:
  Name is $name
  Date is $(date +%F)
EOF

# --- 10.5 Here Document (no expansion) ---
cat <<'NOEXPAND'
  No expansion here: $name $(date)
NOEXPAND

# --- 10.6 Here Document (indented with <<-) ---
	cat <<-INDENTED
	Tabs stripped from the front
	Useful in nested code blocks
	INDENTED

# --- 10.7 Here String ---
read -r first_word <<< "hello world"
echo "Here string first word: $first_word"

# --- 10.8 File Descriptors ---
exec 3>/tmp/bash_demo_fd3.txt    # Open FD 3 for writing
echo "Written to FD 3" >&3
exec 3>&-                         # Close FD 3
cat /tmp/bash_demo_fd3.txt

if [[ -r /etc/hostname ]]; then
    exec 4</etc/hostname              # Open FD 4 for reading
    read -r hostname_val <&4
    exec 4<&-                         # Close FD 4
    echo "Read from FD 4: $hostname_val"
else
    echo "Read from FD 4: (skipped, /etc/hostname not readable)"
fi

# --- 10.9 Pipe ---
echo -e "banana\napple\ncherry" | sort | head -2

# --- 10.10 Process Substitution ---
diff <(echo -e "a\nb\nc") <(echo -e "a\nB\nc") || true

# --- 10.11 tee ---
echo "tee example" | tee /tmp/bash_demo_tee.txt | tr 'a-z' 'A-Z'

# --- 10.12 /dev/null, /dev/zero, /dev/urandom ---
echo "Discarded" > /dev/null
head -c 8 /dev/urandom | od -A n -t x1 | tr -d ' \n'
echo ""

# --- 10.13 Noclobber ---
set +o noclobber  # Default: allow overwriting (set -o noclobber to prevent)

# --- 10.14 Redirecting both stdout and stderr ---
ls / /nonexistent 2>&1 | head -3 || true
ls / /nonexistent &>/tmp/bash_demo_both.txt || true

# Cleanup temp files
rm -f /tmp/bash_demo_*.txt


# =============================================================================
# 11. PIPES & PROCESS MANAGEMENT
# =============================================================================

echo '--- Pipes & Processes ---'

# --- 11.1 Pipe status ---
false | true | false
echo "PIPESTATUS: ${PIPESTATUS[@]}"   # 1 0 1

# --- 11.2 Background processes ---
sleep 0.1 &
bg_pid=$!
echo "Background PID: $bg_pid"
wait "$bg_pid"
echo "Background process finished"

# --- 11.3 Subshell ---
(
    cd /tmp
    echo "In subshell, pwd=$(pwd)"
)
echo "Back in parent, pwd=$(pwd)"

# --- 11.4 Command grouping { } ---
{ echo "Group 1"; echo "Group 2"; } | cat

# --- 11.5 Coproc (Bash 4+) ---
coproc MY_COPROC { cat; }
echo "Hello coproc" >&"${MY_COPROC[1]}"
exec {MY_COPROC[1]}>&-   # Close write end
read -r coproc_reply <&"${MY_COPROC[0]}"
echo "Coproc replied: $coproc_reply"
wait "$MY_COPROC_PID" 2>/dev/null || true

# --- 11.6 xargs ---
echo -e "one\ntwo\nthree" | xargs -I{} echo "  xargs: {}"

# --- 11.7 Command substitution ---
now=$(date +%T)
now_backtick=`date +%T`
echo "Time: $now / $now_backtick"


# =============================================================================
# 12. SPECIAL VARIABLES
# =============================================================================

echo '--- Special Variables ---'
echo "\$0 (script name): $0"
echo "\$# (arg count):   $#"
echo "\$@ (all args):    $@"
echo "\$* (all as one):  $*"
echo "\$? (last exit):   $?"
echo "\$\$ (PID):         $$"
echo "\$! (last bg PID): ${!:-none}"
echo "\$PPID:            $PPID"
echo "\$BASH_VERSION:    $BASH_VERSION"
echo "\$BASH_VERSINFO:   ${BASH_VERSINFO[0]}.${BASH_VERSINFO[1]}.${BASH_VERSINFO[2]}"
echo "\$LINENO:          $LINENO"
echo "\$FUNCNAME:        ${FUNCNAME[0]:-main}"
echo "\$SECONDS:         $SECONDS"
echo "\$RANDOM:          $RANDOM"
echo "\$HOSTNAME:        $HOSTNAME"
echo "\$OSTYPE:          $OSTYPE"
echo "\$MACHTYPE:        $MACHTYPE"
echo "\$SHELL:           $SHELL"
echo "\$UID:             $UID"
echo "\$EUID:            $EUID"
echo "\$PWD:             $PWD"
echo "\$OLDPWD:          ${OLDPWD:-unset}"
echo "\$HOME:            $HOME"
echo "\$IFS repr:        $(printf '%q' "$IFS")"
echo "\$BASHPID:         $BASHPID"
echo "\$BASH_SUBSHELL:   $BASH_SUBSHELL"
echo "\$SHLVL:           $SHLVL"
echo "\$EPOCHSECONDS:    ${EPOCHSECONDS:-N/A}"
echo "\$EPOCHREALTIME:   ${EPOCHREALTIME:-N/A}"
echo "\$SRANDOM:         ${SRANDOM:-N/A}"


# =============================================================================
# 13. BRACE EXPANSION & GLOBBING
# =============================================================================

echo '--- Brace Expansion & Globbing ---'

# --- 13.1 Brace expansion ---
echo "Brace list: {a,b,c} -> " {a,b,c}
echo "Brace range: {1..5} -> " {1..5}
echo "Brace range step: {0..20..5} -> " {0..20..5}
echo "Brace alpha: {a..f} -> " {a..f}
echo "Brace nested: {A{1,2},B{3,4}} -> " {A{1,2},B{3,4}}
echo "Brace prefix: file{1..3}.txt -> " file{1..3}.txt

# --- 13.2 Globbing ---
echo "Glob /etc/host*: " /etc/host*

# --- 13.3 Extended Globbing ---
shopt -s extglob
demo_glob="hello123"
case "$demo_glob" in
    +([a-z])+([0-9])) echo "extglob: letters followed by digits" ;;
esac

# Extended glob patterns:
# ?(pattern)  - zero or one
# *(pattern)  - zero or more
# +(pattern)  - one or more
# @(pattern)  - exactly one
# !(pattern)  - NOT pattern

# --- 13.4 Null Glob ---
shopt -s nullglob
nonexistent_files=(/tmp/this_does_not_exist_xyz_*)
echo "nullglob count: ${#nonexistent_files[@]}"  # 0
shopt -u nullglob

# --- 13.5 Glob Star (Bash 4+) ---
shopt -s globstar
# echo /tmp/**/*.txt  # Would recursively match
shopt -u globstar


# =============================================================================
# 14. SIGNAL HANDLING & TRAPS
# =============================================================================

echo '--- Traps & Signals ---'

# --- 14.1 EXIT trap ---
cleanup() {
    echo "  [trap] Cleanup on EXIT"
}
trap cleanup EXIT

# --- 14.2 ERR trap ---
err_handler() {
    echo "  [trap] Error on line $1, command: $2"
}
trap 'err_handler $LINENO "$BASH_COMMAND"' ERR

# --- 14.3 DEBUG trap ---
# trap 'echo "  [debug] About to run: $BASH_COMMAND"' DEBUG  # Very verbose!

# --- 14.4 RETURN trap ---
trap 'echo "  [trap] Function/source returned"' RETURN

# --- 14.5 Signal traps ---
trap 'echo "  [trap] Caught SIGINT"; exit 130' INT
trap 'echo "  [trap] Caught SIGTERM"; exit 143' TERM
trap 'echo "  [trap] Caught SIGHUP"' HUP

# --- 14.6 Ignore a signal ---
trap '' USR1   # Ignore SIGUSR1

# --- 14.7 List traps ---
trap -p

# Remove ERR/RETURN traps for the rest of the script
trap - ERR
trap - RETURN


# =============================================================================
# 15. PARAMETER EXPANSION (ADVANCED)
# =============================================================================

echo '--- Advanced Parameter Expansion ---'

# --- 15.1 Variable indirection ---
color_red="#FF0000"
ptr="color_red"
echo "Indirect via \${!ptr}: ${!ptr}"

# --- 15.2 Prefix matching ---
color_blue="#0000FF"
color_green="#00FF00"
echo "Variables starting with color_: ${!color_@}"

# --- 15.3 String transformation (Bash 5.1+) ---
test_str="Hello World"
echo "Uppercase: ${test_str@U}" 2>/dev/null || echo "(Requires Bash 5.1+)"
echo "Lowercase: ${test_str@L}" 2>/dev/null || echo "(Requires Bash 5.1+)"
echo "Quote: ${test_str@Q}"
echo "Escape: ${test_str@E}" 2>/dev/null || echo "(Requires Bash 5.1+)"
echo "Attributes: ${counter@a}"    # Shows 'i' for integer
echo "Prompt expand: ${test_str@P}" 2>/dev/null || echo "(Requires Bash 5.1+)"


# =============================================================================
# 16. REGULAR EXPRESSIONS
# =============================================================================

echo '--- Regular Expressions ---'

# --- 16.1 [[ =~ ]] operator ---
email="user@example.com"
if [[ "$email" =~ ^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$ ]]; then
    echo "Valid email: $email"
    echo "Full match: ${BASH_REMATCH[0]}"
fi

# --- 16.2 Capture groups ---
timestamp="2026-03-28 14:30:00"
if [[ "$timestamp" =~ ^([0-9]{4})-([0-9]{2})-([0-9]{2}) ]]; then
    echo "Year=${BASH_REMATCH[1]} Month=${BASH_REMATCH[2]} Day=${BASH_REMATCH[3]}"
fi

# --- 16.3 Using regex in a variable (avoids quoting issues) ---
pattern='^[0-9]+$'
if [[ "12345" =~ $pattern ]]; then
    echo "All digits"
fi


# =============================================================================
# 17. PROCESS & COMMAND CONTROL
# =============================================================================

echo '--- Process Control ---'

# --- 17.1 Command chaining ---
true && echo "AND: first succeeded"
false || echo "OR: first failed"
echo "Semicolon"; echo "chains commands"

# --- 17.2 eval ---
cmd='echo "eval executed"'
eval "$cmd"

# --- 17.3 exec ---
# exec replaces the shell; we demo with redirection only
exec 5>/tmp/bash_exec_demo.txt
echo "Via exec FD5" >&5
exec 5>&-
cat /tmp/bash_exec_demo.txt
rm -f /tmp/bash_exec_demo.txt

# --- 17.4 Command type checking ---
type ls
type -t ls        # file
type -t echo      # builtin
type -t if        # keyword
type -t my_func   # function (if still defined)

# --- 17.5 builtin / command ---
builtin echo "Forced builtin echo"
command ls /dev/null   # Skip aliases/functions


# =============================================================================
# 18. DEBUGGING
# =============================================================================

echo '--- Debugging ---'

# --- 18.1 set flags ---
# set -x   # Print each command before execution (xtrace)
# set -v   # Print shell input lines (verbose)
# set -n   # Read commands but don't execute (noexec / syntax check)
# set -e   # Exit immediately on error
# set -u   # Treat unset variables as error
# set -o pipefail  # Pipe fails if any command fails

# --- 18.2 Selective debugging ---
set -x
echo "This line is traced"
set +x

# --- 18.3 PS4 for trace prefix ---
PS4='+(${BASH_SOURCE}:${LINENO}): ${FUNCNAME[0]:+${FUNCNAME[0]}(): }'
set -x
echo "Detailed trace"
set +x

# --- 18.4 caller ---
trace_caller() {
    echo "caller: $(caller 0)"
}
trace_caller

# --- 18.5 BASH_SOURCE, FUNCNAME, BASH_LINENO ---
show_stack() {
    local frame=0
    while caller $frame; do
        ((frame++))
    done
}
show_stack


# =============================================================================
# 19. ERROR HANDLING PATTERNS
# =============================================================================

echo '--- Error Handling ---'

# --- 19.1 trap ERR (already shown above) ---

# --- 19.2 || with error recovery ---
risky_command() { return 1; }
risky_command || echo "Recovered from failure"

# --- 19.3 Custom die function ---
die() {
    echo "FATAL: $*" >&2
    # exit 1  # Would exit the script
}
# die "Something went wrong"

# --- 19.4 Retry logic ---
retry() {
    local max_attempts=$1; shift
    local attempt=1
    while (( attempt <= max_attempts )); do
        "$@" && return 0
        echo "  Attempt $attempt failed" >&2
        (( attempt++ ))
    done
    return 1
}
flaky() { (( RANDOM % 3 == 0 )); }
retry 10 flaky && echo "Retry succeeded!" || echo "Retry exhausted"

# --- 19.5 Checking command existence ---
require_cmd() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "Required command '$1' not found" >&2
        return 1
    }
}
require_cmd bash && echo "bash is available"


# =============================================================================
# 20. READ & USER INPUT
# =============================================================================

echo '--- Read & Input ---'

# --- 20.1 read with prompt ---
# read -p "Enter your name: " user_name   # Interactive

# --- 20.2 read with timeout ---
if read -t 0.01 -p "" quickvar 2>/dev/null; then
    echo "Got quick input: $quickvar"
else
    echo "read -t timed out (expected in non-interactive mode)"
fi

# --- 20.3 read into array ---
IFS=',' read -ra csv_fields <<< "one,two,three,four"
echo "CSV fields: ${csv_fields[@]}"

# --- 20.4 read specific number of chars ---
read -n 3 three_chars <<< "abcdef" 2>/dev/null
echo "First 3 chars: $three_chars"

# --- 20.5 Silent read (passwords) ---
# read -s -p "Password: " password

# --- 20.6 read from FD ---
exec 6<<< "data from FD 6"
read -r fd6_data <&6
echo "FD6: $fd6_data"

# Reset IFS
IFS=$'\n\t'


# =============================================================================
# 21. PRINTF
# =============================================================================

echo '--- Printf ---'

printf "Padded: %-20s| Done\n" "left-aligned"
printf "Padded: %20s| Done\n" "right-aligned"
printf "Integer: %05d\n" 42
printf "Float: %8.2f\n" 3.14159
printf "Hex: %#x\n" 255
printf "Octal: %#o\n" 255
printf "Char: %c\n" 65
printf "String: %s has %d items\n" "list" 5

# printf -v (assign to variable)
printf -v formatted "Value: %04d" 7
echo "$formatted"

# Repeating pattern
printf '=%.0s' {1..40}
echo ""


# =============================================================================
# 22. GETOPTS (OPTION PARSING)
# =============================================================================

echo '--- Getopts ---'

parse_options() {
    local OPTIND opt verbose=0 output=""
    while getopts ":vo:h" opt; do
        case "$opt" in
            v) verbose=1 ;;
            o) output="$OPTARG" ;;
            h) echo "Usage: script [-v] [-o output] [-h]"; return ;;
            :) echo "Option -$OPTARG requires an argument" >&2; return 1 ;;
            \?) echo "Invalid option: -$OPTARG" >&2; return 1 ;;
        esac
    done
    shift $((OPTIND - 1))
    echo "  verbose=$verbose, output='$output', remaining args: $*"
}
parse_options -v -o "file.txt" extra1 extra2


# =============================================================================
# 23. SHOPT & SET OPTIONS
# =============================================================================

echo '--- Shell Options ---'

# --- 23.1 shopt ---
shopt -s nocasematch     # Case-insensitive matching
if [[ "HELLO" == "hello" ]]; then
    echo "nocasematch: HELLO == hello"
fi
shopt -u nocasematch

shopt -s cdspell         # Auto-correct minor cd typos
shopt -s checkwinsize    # Update LINES/COLUMNS after each command
shopt -s histappend      # Append to history instead of overwriting

# List all shopt options
echo "Some shopt options: $(shopt -p | head -3)"

# --- 23.2 set options ---
echo "Current set options: $-"
# himBHs typically means: h(hashall), i(interactive), m(monitor), B(braceexpand), H(histexpand), s(stdin)


# =============================================================================
# 24. COMPLETION & READLINE
# =============================================================================

echo '--- Completion (non-interactive demo) ---'

# compgen - generate completions
echo "Commands starting with 'ec': $(compgen -c ec | head -3 | tr '\n' ' ')"
echo "Variables starting with 'BAS': $(compgen -v BAS | head -3 | tr '\n' ' ')"
echo "Builtins starting with 'e': $(compgen -b e | tr '\n' ' ')"
echo "Aliases: $(compgen -a 2>/dev/null | head -3 | tr '\n' ' ')"

# complete - would register custom completions in interactive mode
# complete -W "start stop restart" my_service


# =============================================================================
# 25. HISTORY
# =============================================================================

echo '--- History (non-interactive demo) ---'
echo "HISTFILE: ${HISTFILE:-unset}"
echo "HISTSIZE: ${HISTSIZE:-unset}"
echo "HISTCONTROL: ${HISTCONTROL:-unset}"
echo "HISTIGNORE: ${HISTIGNORE:-unset}"
echo "HISTTIMEFORMAT: ${HISTTIMEFORMAT:-unset}"


# =============================================================================
# 26. SOURCING & LOADING
# =============================================================================

echo '--- Sourcing ---'

# --- 26.1 source / . ---
tmpscript=$(mktemp)
echo 'SOURCED_VAR="I was sourced"' > "$tmpscript"
source "$tmpscript"
echo "$SOURCED_VAR"
rm -f "$tmpscript"

# --- 26.2 Detecting if sourced vs executed ---
# (( ${#BASH_SOURCE[@]} > 1 )) && echo "Sourced" || echo "Executed"


# =============================================================================
# 27. DATE & TIME
# =============================================================================

echo '--- Date & Time ---'

echo "Epoch seconds: ${EPOCHSECONDS:-$(date +%s)}"
echo "Epoch real: ${EPOCHREALTIME:-N/A}"
echo "SECONDS (runtime): $SECONDS"
echo "date formats: $(date '+%Y-%m-%d %H:%M:%S %Z')"
echo "date ISO: $(date -Iseconds 2>/dev/null || date '+%Y-%m-%dT%H:%M:%S%z')"

# TIMEFORMAT for timing
TIMEFORMAT='  real=%Rs user=%Us sys=%Ss'
time { sleep 0.01; } 2>&1


# =============================================================================
# 28. NETWORKING (BASH BUILT-IN)
# =============================================================================

echo '--- Bash Networking ---'

# --- 28.1 /dev/tcp (Bash built-in) ---
# Uncomment to test (requires network):
# exec 7<>/dev/tcp/example.com/80
# echo -e "GET / HTTP/1.0\r\nHost: example.com\r\n" >&7
# cat <&7
# exec 7>&-

# --- 28.2 /dev/udp ---
# exec 7<>/dev/udp/localhost/12345
# echo "hello" >&7
# exec 7>&-

echo "(Network features available but skipped in demo)"


# =============================================================================
# 29. MISCELLANEOUS FEATURES
# =============================================================================

echo '--- Misc Features ---'

# --- 29.1 Arithmetic for loop with multiple variables ---
for ((i=0, j=10; i<5; i++, j-=2)); do
    echo "  i=$i, j=$j"
done

# --- 29.2 Mapfile with callback ---
process_line() { echo "  mapfile cb: $2"; }
mapfile -t -C process_line -c 1 <<< $'x\ny\nz'

# --- 29.3 printf %q (shell-escape) ---
dangerous='hello "world" $(rm -rf /)'
printf "Safe: %q\n" "$dangerous"

# --- 29.4 Bash loadable builtins ---
# enable -f /usr/lib/bash/sleep sleep  # Load builtin version

# --- 29.5 Hash table (command cache) ---
hash -r          # Clear command hash table
hash ls 2>/dev/null  # Cache ls path
hash -l 2>/dev/null | head -3  # List cached

# --- 29.6 ulimit ---
echo "Max open files: $(ulimit -n)"
echo "Max processes: $(ulimit -u)"

# --- 29.7 umask ---
echo "Current umask: $(umask)"
echo "Symbolic umask: $(umask -S)"

# --- 29.8 wait ---
sleep 0.05 & p1=$!
sleep 0.05 & p2=$!
wait "$p1" "$p2"
echo "Both background jobs done"

# --- 29.9 disown ---
sleep 0.01 &
disown $!
echo "Disowned background process"

# --- 29.10 String repetition trick ---
printf '%*s' 30 '' | tr ' ' '-'
echo ""

# --- 29.11 Enable / disable builtins ---
enable -n test 2>/dev/null || true   # Disable 'test' builtin
enable test 2>/dev/null || true      # Re-enable it

# --- 29.12 Subshell variable isolation ---
(
    isolated="can't escape"
    echo "  Inside subshell: $isolated"
)
echo "  Outside subshell: ${isolated:-empty}"

# --- 29.13 Temporary FD with automatic close ---
{
    echo "Auto-close FD block"
} > /dev/null


# =============================================================================
# 30. ADVANCED PATTERNS
# =============================================================================

echo '--- Advanced Patterns ---'

# --- 30.1 Mutex / lock file ---
lock() {
    local lockfile="/tmp/bash_demo.lock"
    if ( set -o noclobber; echo $$ > "$lockfile" ) 2>/dev/null; then
        trap "rm -f '$lockfile'" EXIT
        echo "  Lock acquired (PID $$)"
        return 0
    else
        echo "  Lock already held by $(cat "$lockfile" 2>/dev/null)"
        return 1
    fi
}
lock

# --- 30.2 Dynamic function creation ---
for cmd in start stop restart; do
    eval "service_${cmd}() { echo \"  Service action: $cmd\"; }"
done
service_start
service_stop
service_restart

# --- 30.3 Stack implementation ---
declare -a stack=()
stack_push() { stack+=("$1"); }
stack_pop()  {
    local top=${stack[-1]}
    unset 'stack[-1]'
    echo "$top"
}
stack_push "first"
stack_push "second"
stack_push "third"
echo "  Pop: $(stack_pop)"
echo "  Pop: $(stack_pop)"
echo "  Stack: ${stack[*]}"

# --- 30.4 Simple key-value store ---
declare -A kv_store
kv_set() { kv_store["$1"]="$2"; }
kv_get() { echo "${kv_store[$1]:-}"; }
kv_set "user" "Alice"
kv_set "role" "admin"
echo "  KV user=$(kv_get user), role=$(kv_get role)"

# --- 30.5 Parallel execution with wait ---
parallel_task() { sleep 0.0$((RANDOM % 5)); echo "  Task $1 done"; }
pids=()
for i in {1..3}; do
    parallel_task "$i" &
    pids+=($!)
done
for pid in "${pids[@]}"; do wait "$pid"; done
echo "  All parallel tasks complete"

# --- 30.6 Self-modifying data with heredoc + eval ---
config=$(cat <<'CFG'
APP_NAME="DemoApp"
APP_PORT=8080
CFG
)
eval "$config"
echo "  Config: $APP_NAME on port $APP_PORT"


# =============================================================================
# SUMMARY
# =============================================================================

cat <<'SUMMARY'

╔══════════════════════════════════════════════════════════════╗
║              BASH FEATURES COVERED (30 SECTIONS)            ║
╠══════════════════════════════════════════════════════════════╣
║  1. Shebang & Comments        16. Regular Expressions       ║
║  2. Variables & Data Types    17. Process & Command Control  ║
║  3. Quoting                   18. Debugging                  ║
║  4. Arrays (indexed+assoc)    19. Error Handling Patterns    ║
║  5. String Operations         20. Read & User Input          ║
║  6. Arithmetic                21. Printf                     ║
║  7. Conditionals              22. Getopts (option parsing)   ║
║  8. Loops                     23. Shopt & Set Options        ║
║  9. Functions                 24. Completion & Readline       ║
║ 10. I/O & Redirection        25. History                     ║
║ 11. Pipes & Processes         26. Sourcing & Loading         ║
║ 12. Special Variables         27. Date & Time                ║
║ 13. Brace Expansion/Globbing  28. Networking (built-in)      ║
║ 14. Signal Handling & Traps   29. Miscellaneous Features     ║
║ 15. Advanced Param Expansion  30. Advanced Patterns          ║
╚══════════════════════════════════════════════════════════════╝
SUMMARY

echo "Script completed in ${SECONDS}s."
