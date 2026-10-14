# Highlight rule `where` on Linux: manual checks

Replaces the skipped item 281 of `../linux-test.md` ("Highlight rule `where`. Not bound yet"). Numbering goes on after `16-linux.md` (296) and the items another branch takes (297–306). Results go to `../linux-test-results.md`.

307. **A `where` rule colors rows.** Make a folder with a few sound files of different lengths (for example `ffmpeg -f lavfi -i sine=d=10 long.wav` and `ffmpeg -f lavfi -i sine=d=1 short.wav`, or songs of your own), a `.txt` file and a subfolder. In `~/.config/gezik/settings.toml` add, in this order:
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
308. **No reading without `where`.** Remove the first rule (keep the `kind = "audio"` one) and open a folder of a few thousand files: `top` shows no extra CPU from Gezik after the list shows. Put the `where` rule back: only the rows on screen are read (CPU settles once they have their colors).
309. **A wrong `where`.** Add `[[highlight-rules]] where = "duration>3x"` with `color = "green"`: a warning names `highlight-rules[3]: where: duration>3x: …`; the other rules still color rows.
