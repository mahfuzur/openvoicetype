#!/usr/bin/env bash
# Writes golden/text.json: each case of golden/text-inputs.json run through dictate.sh's own text functions (taken out
# of the script with sed, not copied), with the script's output as "expected". ovt-pipeline's text.rs must give the same
# (tests/golden_text.rs). Run it after changing those functions in dictate.sh or the inputs, and commit both files:
#
#   bash crates/ovt-pipeline/golden/make-text.sh
#
# Deterministic on macOS (bash 3.2 too) and Linux: the C locale (the apps start the script without LANG) and UTC.
# format_resets cases must not fall on today (the script compares with the current date); the generator checks.
# Keep non-ASCII whitespace out of word_count cases: Ubuntu 25.10's Rust coreutils `wc -w` splits at a non-breaking
# space even in the C locale (GNU and BSD wc don't; text.rs follows them).
set -uo pipefail
export LC_ALL=C TZ=UTC

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
D="$REPO/scripts/dictate.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cd "$REPO" || exit 1

# The functions under test, and the constants they use.
eval "$(sed -n '/^load_dictionary()/,/^}/p; /^trim()/,/^}/p; /^vocabulary()/,/^}/p; /^whisper_prompt()/,/^}/p
  /^system_prompt()/,/^}/p; /^user_message()/,/^}/p; /^post_process()/,/^}/p; /^meaning_guard()/,/^}/p
  /^s1_control_line()/,/^}/p; /^format_resets()/,/^}/p; /^command_message()/,/^}/p; /^command_source()/,/^}/p
  /^word_count()/p; /^log_safe()/p' "$D")"
eval "$(sed -n "/^FALLBACK_PROMPT=/,/final text\.'\$/p; /^HALLUCINATIONS=/p" "$D")"
unset WHISPER_STYLE
eval "$(sed -n '/^WHISPER_STYLE=/p' "$D")"
DEFAULT_STYLE="$WHISPER_STYLE"
# transcribe's clean-up of Whisper's output (its lines from "Join lines" to the hallucination check's "fi").
JOIN="$(sed -n '/# Join lines and trim whitespace/,/^  fi$/p' "$D")"
clean_transcript() {
  local out
  out="$(printf '%s' "$1")" # transcribe has it from $(…)
  eval "$JOIN"
  printf '%s' "$out"
}
for f in load_dictionary trim vocabulary whisper_prompt system_prompt user_message post_process meaning_guard \
  s1_control_line format_resets command_message command_source word_count log_safe; do
  declare -F "$f" >/dev/null || { echo "make-text.sh: $f not found in $D" >&2; exit 1; }
done
[[ -n "$FALLBACK_PROMPT" && -n "$HALLUCINATIONS" && -n "$DEFAULT_STYLE" && "$JOIN" == *HALLUCINATIONS*fi ]] ||
  { echo "make-text.sh: a constant or transcribe's clean-up wasn't found in $D" >&2; exit 1; }

# The script's variables, as dictate.sh sets them by default (a case's "vars" override them).
# shellcheck disable=SC2034 # read by the functions eval'd above
reset_vars() {
  JOB=cleanup MODE=default APP_NAME="" VOCAB="" VTT_VOCAB="" WHISPER_PROMPT=on WHISPER_STYLE="$DEFAULT_STYLE"
  LOG_TEXT=off PROMPTS_DIR="$REPO/prompts" PROMPT_FILE="$WORK/none/prompt.txt"
  DICTIONARY_FILE="$WORK/none/dictionary.txt" COMMAND_FILE=""
}

# Runs one case and writes its output to out/<id> and the exit status to out/<id>.status, the way the script's
# callers capture each function (most through $(…), which drops trailing newlines).
run_case() {
  local id="$1" fn="$2" out="" status=0
  shift 2
  load_dictionary
  case "$fn" in
    trim | word_count | format_resets | log_safe | meaning_guard) out="$("$fn" "$@")" || status=$? ;;
    vocabulary | whisper_prompt | system_prompt | command_source | s1_control_line) out="$("$fn")" || status=$? ;;
    user_message | command_message) # piped as it is by the one-shot call: keep the final newline
      out="$("$fn" "$1" || exit; printf x)" || status=$?
      out="${out%x}" ;;
    post_process) out="$(printf '%s' "$1" | post_process)" || status=$? ;;
    clean_transcript) out="$(clean_transcript "$1")" || status=$? ;;
    is_hallucination) if printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | grep -Eq "$HALLUCINATIONS"; then out=true; else out=false; fi ;;
    load_dictionary) out="$(printf '%s\036' "${DICT_TERMS[@]+"${DICT_TERMS[@]}"}"; printf '\035%s' "$REPLACEMENTS")" ;;
    *) echo "make-text.sh: unknown function $fn" >&2; status=99 ;;
  esac
  printf '%s' "$out" >"$WORK/out/$id"
  echo "$status" >"$WORK/out/$id.status"
}

# The cases as a bash script: per case, its files (dictionary, prompt file, command file) and its variables.
mkdir -p "$WORK/out"
perl -MJSON::PP -e '
  my ($inputs, $work) = @ARGV;
  open my $in, "<", $inputs or die "$inputs: $!";
  my $cases = JSON::PP->new->utf8->decode(do { local $/; <$in> });
  sub quote { my $text = shift // ""; $text =~ s/\x27/\x27\\\x27\x27/g; "\x27$text\x27" }
  sub write_file { my ($path, $text) = @_; open my $out, ">:encoding(UTF-8)", $path or die "$path: $!"; print $out $text; close $out }
  binmode STDOUT, ":encoding(UTF-8)";
  my %allowed = map { $_ => 1 } qw(JOB MODE APP_NAME VOCAB VTT_VOCAB WHISPER_PROMPT WHISPER_STYLE LOG_TEXT);
  for my $id (0 .. $#$cases) {
    my $case = $cases->[$id];
    my $dir = "$work/case-$id";
    mkdir $dir or die "$dir: $!";
    my @set = ("reset_vars");
    for my $name (sort keys %{ $case->{vars} // {} }) {
      die "case $case->{name}: unknown variable $name\n" unless $allowed{$name};
      push @set, "$name=" . quote($case->{vars}{$name});
    }
    if (defined $case->{dictionary}) { write_file("$dir/dictionary.txt", $case->{dictionary}); push @set, "DICTIONARY_FILE=" . quote("$dir/dictionary.txt") }
    if (defined $case->{prompt_file}) { write_file("$dir/prompt.txt", $case->{prompt_file}); push @set, "PROMPT_FILE=" . quote("$dir/prompt.txt") }
    push @set, "PROMPTS_DIR=\"\$REPO\"/" . quote($case->{prompts_dir}) if defined $case->{prompts_dir};
    if (exists $case->{command}) {
      my $command = $case->{command};
      write_file("$dir/command.json", ref $command ? JSON::PP->new->canonical->encode($command) : $command);
      push @set, "COMMAND_FILE=" . quote("$dir/command.json");
    }
    push @set, "COMMAND_FILE=" . quote("$dir/missing.json") if $case->{command_missing};
    print "(\n  ", join("\n  ", @set), "\n  run_case $id ", join(" ", map { quote($_) } $case->{fn}, @{ $case->{args} // [] }), "\n)\n";
  }' "$HERE/text-inputs.json" "$WORK" >"$WORK/cases.sh" || exit 1
# shellcheck source=/dev/null
source "$WORK/cases.sh"

# The inputs with each one's output, as JSON.
perl -MJSON::PP -e '
  my ($inputs, $work, $output) = @ARGV;
  open my $in, "<", $inputs or die "$inputs: $!";
  my $cases = JSON::PP->new->utf8->decode(do { local $/; <$in> });
  my $failed = 0;
  for my $id (0 .. $#$cases) {
    my $case = $cases->[$id];
    open my $file, "<:encoding(UTF-8)", "$work/out/$id" or die "case $id: $!";
    my $out = do { local $/; <$file> } // "";
    open my $status_file, "<", "$work/out/$id.status" or die "case $id: $!";
    chomp(my $status = <$status_file>);
    my $fn = $case->{fn};
    if ($fn eq "meaning_guard") { # exit 1 with the reason, or 0
      if ($status > 1) { warn "case $case->{name}: $fn exited with $status\n"; $failed++; next }
      $case->{expected} = $status ? $out : undef;
      next;
    }
    if ($status) { warn "case $case->{name}: $fn exited with $status\n"; $failed++; next }
    if ($fn eq "is_hallucination") {
      $case->{expected} = $out eq "true" ? JSON::PP::true : JSON::PP::false;
    } elsif ($fn eq "load_dictionary") {
      my ($terms, $replacements) = split /\x1d/, $out, 2;
      $case->{expected} = { terms => [split /\x1e/, $terms], replacements => [split /\n/, $replacements // ""] };
    } else {
      $case->{expected} = $out;
      if ($fn eq "format_resets" && $case->{args}[0] =~ /^[0-9]+$/ && $out !~ /,/) {
        warn "case $case->{name}: $out is today, pick another time\n";
        $failed++;
      }
    }
  }
  exit 1 if $failed;
  open my $json, ">", $output or die "$output: $!";
  print $json JSON::PP->new->utf8->canonical->pretty->encode($cases);
  printf "make-text.sh: %d cases written to %s\n", scalar @$cases, $output;' \
  "$HERE/text-inputs.json" "$WORK" "$HERE/text.json"
