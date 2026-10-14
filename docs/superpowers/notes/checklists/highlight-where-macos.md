# Highlight rule `where` on macOS: manual checks

Replaces the skipped item 387 of `../macos-test.md` ("Highlight rule `where`. Not bound yet"). Numbering goes on after `16-macos.md` (412) and the items another branch takes (413–422). Results go to `../macos-test-results.md`.

423. **A `where` rule colors rows.** Make a folder with a few sound files of different lengths (`say -o long.aiff "…a long sentence…"` for one over 3 seconds, `say -o short.aiff hi` for one under), an `.mp3` or `.m4a` of your own, a `.txt` file and a subfolder. In `settings.toml` add, in this order:
    ```toml
    [[highlight-rules]]
    where = "duration>3s"
    color = "red"
    bold = true

    [[highlight-rules]]
    kind = "audio"
    color = "blue"
    ```
    Save and open the folder: the sound files longer than 3 seconds are red and bold, the shorter ones blue; the `.txt` file and the subfolder keep the theme's color. Scrolling a long folder of songs: rows coming into view get their color a moment after they show, never blue first and then red.
424. **No reading without `where`.** Remove the first rule (keep the `kind = "audio"` one) and open a folder of a few thousand files: Activity Monitor shows no extra CPU from Gezik after the list shows (no properties are read). Put the `where` rule back: only the rows on screen are read (CPU settles once they have their colors).
425. **A wrong `where`.** Add `[[highlight-rules]] where = "duration>3x"` with `color = "green"`: a warning names `highlight-rules[3]: where: duration>3x: …`; the other rules still color rows.
