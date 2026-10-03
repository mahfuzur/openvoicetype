#!/usr/bin/env bash
# Writes golden/claude.json: dictate.sh's own claude_parse run over the event streams in golden/claude-inputs.json, with
# its exit code, its output, what it wrote to ENGINE_ERR_FILE (fields separated by \x1f) and its log lines (the time
# replaced by "<time>"). ovt-pipeline's claude::parse must give the same (src/claude.rs, parse_matches_the_script).
# Runs with macOS's bash 3.2 and perl's JSON::PP; the output must be the same on macOS and Linux.
set -euo pipefail
cd "$(dirname "$0")/../../.."
golden=crates/ovt-pipeline/golden
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

eval "$(sed -n '/^claude_parse()/,/^}/p' scripts/dictate.sh)"

# One file per case: its name and its events (an object line is written compact, with sorted keys).
count="$(perl -MJSON::PP -e '
  local $/; open my $f, "<", $ARGV[0] or die; my $in = JSON::PP->new->utf8->decode(<$f>);
  my $json = JSON::PP->new->utf8->canonical;
  my $i = 0;
  for my $case (@{ $in->{cases} }) {
    open my $name, ">:encoding(UTF-8)", "$ARGV[1]/$i.name" or die; print $name $case->{name}; close $name;
    open my $events, ">", "$ARGV[1]/$i.events" or die;
    print $events (ref $_ ? $json->encode($_) : do { my $s = $_; utf8::encode($s); $s }), "\n" for @{ $case->{events} };
    close $events;
    $i++;
  }
  print $i;' "$golden/claude-inputs.json" "$work")"

i=0
while ((i < count)); do
  ENGINE_ERR_FILE="$work/$i.err" LOG_FILE="$work/$i.log"
  status=0
  claude_parse <"$work/$i.events" >"$work/$i.out" || status=$?
  printf '%s' "$status" >"$work/$i.status"
  i=$((i + 1))
done

perl -MJSON::PP -e '
  my ($dir, $count) = @ARGV;
  my $read = sub { my $path = shift; open my $f, "<:raw", $path or return ""; local $/; my $s = <$f>; utf8::decode($s); $s };
  my @cases;
  for my $i (0 .. $count - 1) {
    (my $log = $read->("$dir/$i.log")) =~ s/^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d /<time> /mg;
    push @cases, {
      name => $read->("$dir/$i.name"),
      exit => 0 + $read->("$dir/$i.status"),
      stdout => $read->("$dir/$i.out"),
      error => $read->("$dir/$i.err"),
      log => $log,
    };
  }
  print JSON::PP->new->utf8->canonical->pretty->encode({
    about => "Made by golden/make-claude.sh from dictate.sh claude_parse: do not edit.",
    cases => \@cases,
  });' "$work" "$count" >"$golden/claude.json"
echo "wrote $golden/claude.json ($count cases)"
