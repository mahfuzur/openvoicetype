import Foundation

// ovt-apple: Apple's on-device engines for OpenVoiceType (macOS 26+). Run by dictate.sh; audio and text stay on the Mac.
//
//   ovt-apple status [--locale en] [--transcriber speech|dictation]
//       One JSON line: {"speech":{"available":…,"locale":…,"assets":…},"llm":{"available":…,"reason":…}}
//   ovt-apple speech-install [--locale en] [--transcriber …]
//       Downloads the speech model: `progress <0-1>` lines, then `installed`.
//   ovt-apple transcribe [--locale en] [--transcriber speech|dictation] [--vocab "a, b"] <wav>
//   ovt-apple transcribe … --pcm [--stats-file F]
//       Prints the transcript. --pcm reads 16 kHz mono 16-bit PCM from stdin while it arrives (live) and finishes at
//       end of input; the stats file gets "<audio seconds> <ms from end of input to text>".
//   ovt-apple cleanup --instructions-file F
//       Prewarms Apple's language model with the instructions, reads the user message on stdin, prints the answer.
//
// Failures: exit 1 (transcription) or 2 (cleanup), with one line on stderr. For cleanup it starts with the kind:
// unavailable <reason>, refusal, guardrail, context, rate-limited or error <detail>.

struct Options {
    var command = ""
    var locale = "en"
    var transcriber = TranscriberKind.speech
    var vocabulary: [String] = []
    var pcm = false
    var statsFile: String?
    var instructionsFile: String?
    var file: String?

    init(_ arguments: [String]) throws {
        var rest = arguments[...]
        command = rest.popFirst() ?? ""
        while let argument = rest.popFirst() {
            func value() throws -> String {
                guard let value = rest.popFirst() else { throw HelperError("\(argument) needs a value") }
                return value
            }
            switch argument {
            case "--locale": locale = try value()
            case "--transcriber":
                let name = try value()
                guard let kind = TranscriberKind(rawValue: name) else { throw HelperError("unknown transcriber \(name)") }
                transcriber = kind
            case "--vocab":
                vocabulary = try value().split(separator: ",")
                    .map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
            case "--pcm": pcm = true
            case "--stats-file": statsFile = try value()
            case "--instructions-file": instructionsFile = try value()
            default:
                guard !argument.hasPrefix("--"), file == nil else { throw HelperError("unknown option \(argument)") }
                file = argument
            }
        }
    }
}

struct HelperError: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}

func fail(_ message: String, status: Int32) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8))
    exit(status)
}

func printLine(_ line: String) {
    FileHandle.standardOutput.write(Data((line + "\n").utf8))
}

let options: Options
do {
    options = try Options(Array(CommandLine.arguments.dropFirst()))
} catch {
    fail("error \(error)", status: 64)
}

switch options.command {
case "status":
    await printLine(Status.json(options))
case "speech-install":
    do {
        try await SpeechEngine.install(options)
        printLine("installed")
    } catch {
        fail("error \(error)", status: 1)
    }
case "transcribe":
    do {
        let text = try await SpeechEngine.transcribe(options)
        FileHandle.standardOutput.write(Data(text.utf8))
    } catch {
        fail("error \(error)", status: 1)
    }
case "cleanup":
    await Cleanup.run(options)
default:
    fail("usage: ovt-apple status|speech-install|transcribe|cleanup [options]", status: 64)
}
