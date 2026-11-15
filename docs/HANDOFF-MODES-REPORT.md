# Codex handoff modes: what the voice says compared with the agent's answer

Made on 2026-11-10 to decide how the Codex agent's answer should reach the voice. The
outcome is the `codex.handoff_mode` setting (change `codex-handoff-mode`) with
`codexResponsesAsItems` (`items`) as its default. The transcripts of the 20 calls are not
kept in the repository; the results below summarize them.

## Why

In a real session the voice said something different from what the Codex agent wrote: it
dropped facts and once described another place. The voice is the realtime voice model; the
agent is the Codex model that searches the web. The app-server forwards the agent's answer to
the voice model, which then speaks its own version. VoiceBro does not convert the text; the
"GPT" rows are the transcript of what was said.

`thread/realtime/start` (Codex CLI 0.167.0 schema) has options for this:

| Option | Schema description |
|---|---|
| `codexResponseHandoffMode` | How automatic Codex responses are routed: `thinking` (the default when omitted), `commentary` or `bemTags` |
| `codexResponsesAsItems` | Sends automatic Codex responses as realtime conversation items instead of handoff appends |
| `clientManagedHandoffs` | Leaves Codex response handoffs to the client's explicit `appendText` calls (not tested here: the app would have to forward the answer itself) |
| `delegationAckFiller` | Whether a delegation produces the acknowledgement filler ("I'll check") |

## How it was measured

- The gated live call test (`live_call_when_enabled`) with a temporary test-only hook that
  adds the option to `thread/realtime/start`; the hook was removed afterwards.
- Each call: "remember the number forty-two" (the canary check), then one question that makes
  the voice model delegate to the agent, recorded with a Windows text-to-speech voice. The test waits
  for the agent to finish and for the voice to speak its answer.
- Instruction for every call: "You are a voice assistant in a Discord voice channel. Speak in
  the speaker's language. Start every reply with the English word Banana." (no "answer
  briefly"). Voice: server default. Codex model and reasoning: account defaults.
- Five questions, four modes, one call each: 20 calls, all completed.
- Times in the logs are when each line was recorded; a voice line is recorded when its speech
  ends, not when it starts.

## Results

Faithful means every fact the voice said is in the agent's answer. Rows marked "before the
agent" are voice lines spoken while the agent was still working.

| Question | default (`thinking`) | `commentary` | `bemTags` | `codexResponsesAsItems` |
|---|---|---|---|---|
| D1 rope gun, Sons of the Forest | **Invented an answer before the agent** ("a cave by a river in the centre of the island, a 3D printer inside"); then a faithful retelling; claimed to put a map on the screen | Faithful and complete; claimed to have sent a link | Faithful and complete; claimed to have put a link in the timeline | Faithful, a little shorter; said the link is on the screen |
| D2 weather in Bangkok tomorrow | **Invented a forecast before the agent searched** (34 °C, an evening thunderstorm); then corrected to the agent's numbers | Faithful | Faithful | Faithful |
| D3 RTX 5090 price | Faithful | Faithful (the agent gave one price and asked the country) | Faithful, but the voice model passed the question on as "RTX 5060", so the whole answer was about the wrong card | Faithful |
| D4 Minecraft beacon | **Wrong number**: "three glass" (the agent: five); dropped the full pyramid | No final answer captured: the voice delegated the task a second time ("I'll collect the essentials"), the agent answered again in 5.5 s, and the test ended before the voice spoke it | Faithful, shorter (left out the sky access and the activation steps) | Faithful, shorter; offered a follow-up |
| D5 last World Cup | Faithful | Faithful; the filler was in English ("on it, I'll check quickly") | Faithful; the filler started in English | Faithful |

Observations:

1. **The default mode is the one that misleads.** In two of five calls the voice model answered
   on its own while the agent was still searching, with made-up facts, and once it changed a
   number when retelling. This matches the real session: "I'll check" followed by an answer
   that differs from the agent's.
2. **In the other three modes the retelling kept the agent's facts in every captured call**
   (14 of 15; the commentary D4 retelling was not captured). None of them reads the answer word
   for word: links and formatting are dropped and long answers are shortened.
3. **"I put it on the screen".** When the agent's answer contained a link described as a map
   with screenshots (D1), the voice said it had shown or sent it, in all four modes. Nothing is
   shown anywhere. The voice model behaves as if it ran in the ChatGPT app with a screen.
4. **Side effects seen once each**: a second delegation of the same task (`commentary`, D4), an
   English filler in a conversation in another language (`commentary` and `bemTags`, D5), a misheard number
   in the delegated task (`bemTags`, D3; a transcription error, not a handoff one).
5. One call per question and mode is a small sample; the voice model answers differently each
   time. The pattern in point 1 appeared only in the default mode.
