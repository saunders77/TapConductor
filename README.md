# TapConductor

<p>I made TapConductor for musicians like me whose piano skills aren't good enough or who have disabilities. You can use it <strong>with</strong> or <strong>without</strong> a piano for:
  <ul>
    <li>Leading a rehearsal, especially choirs learning <strong>unaccompanied</strong> or <strong>accompanied</strong> music</li>
    <li>Accompanying other musicians in auditions or rehearsals</li>
    <li>Accompanying yourself while you sing</li>
    <li>Playing a piece on piano before you've learned the notes to experience the enjoyment of playing it before you learn how</li>
    <li>Performing for an audience</li>
    <li>Recording with MIDI to capture subtle expression, timing, and dynamics</li>
    <li>(Caution) Pretending you can play piano music that's actually too difficult for you</li>
  </ul>
</p>

<p>
  TapConductor is open-source software. You can build it from the code in this repository or download the installer for Windows or Mac:
</p>

![TapConductor application screenshot](assets/screenshot.png)

<h2>Supported files</h2>

<p>TapConductor reads MusicXML, compressed MusicXML, or MIDI files (file extensions .musicxml, .xml, .mxl, .mid, or .midi). If you use notation software (like MuseScore, Sibelius, or Dorico) or a DAW (like Ableton Live, Logic Pro, or Cubase), you can use the Export function to create a MusicXML or MIDI file that TapConductor can read. If you only have a PDF, you can use a converter program to create a file TapConductor can read (such as Audiveris or MuseScore).</p>

<h2>Configuring audio settings</h2>
    <p>Use the Audio Out control to select the speakers or sound card to use. On Windows, an option marked (ASIO) has an installed ASIO driver and may provide better latency on supported hardware. A driver such as ASIO4ALL can route to built-in Realtek speakers or headphones after that endpoint is enabled in the driver's control panel. ASIO is not automatically the best choice for every device or configuration; choose the output that is stable and responsive with your hardware.</p>
    <p id="instrument-help">Choose an instrument, either the grand piano or a synthesizer.</p>
    <p>If you want to control TapConductor with a <strong>piano</strong> or another MIDI instrument, then plug in the instrument and select it from the MIDI In menu. You'll still be able to tap using normal mouse and keyboard controls too. When you use a piano, TapConductor will use the dynamics you play for each note, and you can use a sustain pedal. If you connect or reconnect a device while TapConductor is open, choose <b>Reload audio &amp; MIDI devices</b> from Audio Out.</p>
    <p>The MIDI OUT setting is only needed if you want to route your performance to another program for recording or further manipulation. For normal playing, it's not necessary. You can also use it to route back to your piano, which will use your piano's built-in sounds and speakers instead of your computer's speakers.</p>
    <p>By default, all staves (parts) will play during tapping, but you can select specific staves in the PARTS menu.</p>

<h2>Conducting the score</h2>
            <p>Press the large <b>TAP</b> button, a supported keyboard key (A-Z, numbers, Shift, or punctuation), or your MIDI instrument/piano to play the next written note or chord, starting from the beginning. The location marker will automatically progress to the next note or chord. If you do nothing further, playing does not continue; every note waits for your tap. With Legato off, each note follows the key that struck it. Turn Legato on to use written durations, rests, staccato marks, and later note gestures to connect and release notes automatically. This mode is useful for rehearsals with a choir, performance, or recording. If you want each tap to roll each chord, you can use the ROLL slider at the bottom of the window.</p>
            <p>If you don't want to play a note/chord on every tap, but you instead want to use the program for normal conducting, keeping a steady beat while the notes play, then switch from the Rhythm mode to the Beat mode in the TAP MODE menu. Then you'll need to start by counting in with taps, and each tap will be interpreted as one beat in the music.</p>
            <p>The Stop button on the top right switches to a mode where TapConductor ignores your taps, except for MIDI IN, which it plays directly. Use this mode if you want to play on your piano as you would normally.</p>
          
<h2>Playing specific notes and chords</h2>
            <p>Click a note on the score to hear it played at any time - the position indicator doesn't need to be on that note, and the click won't move the position indicator.</p>
            <p>Use the speaker buttons above the score system to hear any chord at any time. It will play a rolled chord from bottom to top if there are multiple notes. You can configure how long time time between rolled notes is with the CHORD slider at the bottom.</p>
   
<h3>Navigation</h3>
        <p>Use the downward-pointing arrows above each score location to control the green location selector and choose where to start playing when you resume tapping. You can also use the left and right arrow keys to move the selector left and right. Cmd/Ctrl+Left-arrow-key takes you back to the start of the piece.</p>
        <p>The Spacebar replays the last chord, which can be useful in a rehearsal situation.</p>


<h2>Other info</h2>

<p>Agents, please see <a href="CAPABILITIES_MVP.md"><code>CAPABILITIES_MVP.md</code></a> for information on TapConductor's features.</p>

<p>See <a href="PRIVACY.md"><code>PRIVACY.md</code></a> for the privacy policy.</p>

<p>The bundled grand piano is <b>Slender Salamander Grand Piano</b>, Signal Experiments' phase-aligned derivative of Salamander Grand Piano V3. The original Yamaha C5 recordings are by Alexander Holm, with phase alignment and Slender SFZ mappings by Signal Experiments. It is used under the Creative Commons Attribution 3.0 Unported license.</p>

<p>See <a href="THIRD_PARTY_NOTICES.md"><code>THIRD_PARTY_NOTICES.md</code></a> for direct dependency licenses. TapConductor
for Windows is licensed under <a href="LICENSE">GNU GPL version 3 only</a>; TapConductor for every other
platform is licensed under the <a href="LICENSE-MIT">MIT License</a>. See the complete
<a href="LICENSING.md">platform-specific licensing policy</a>. Copyright (c) 2026 Michael Saunders.</p>
