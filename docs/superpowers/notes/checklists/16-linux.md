# Step 16 on Linux: manual checks

Numbering goes on from `15-linux.md` (it ends at 285). Results go to `../linux-test-results.md`.

### 16a, inside archives

286. **Zip, tar.xz and deb as folders.** Double-click a `.zip`, a `.tar.xz` and a `.deb` (any from `/var/cache/apt/archives` or `apt download hello`): each opens as a folder (the deb shows `DEBIAN` and `usr`), sort and the filter (Ctrl+F) work inside. Drag a file from inside the zip onto a Nautilus window (GNOME, X11 and Wayland) and onto Dolphin (KDE): it is copied there. Ctrl+C inside the zip, Ctrl+V in another tab's folder: copied, Ctrl+Z removes it.
287. **Formats only 7-Zip reads.** A `.wim` or `.dmg` (or an `.msi`) with 7-Zip installed (`7zz` 25 or newer): double-click shows its contents. Without 7-Zip (rename it away, or `[tools] seven-zip` pointing nowhere): the box offering 7-Zip appears; after the download the archive opens.

### 16b, writing to archives

288. **Add to and delete from a tar.gz.** Make one with `tar czf t.tar.gz somefolder` and open it in Gezik. Drag a file from Nautilus (GNOME on Wayland) and from Dolphin (KDE) into its list; paste another with Ctrl+V; delete one inside (Delete, confirm). `tar tzvf t.tar.gz` lists exactly what Gezik shows, with the old entries' dates and permissions unchanged. Ctrl+Z three times: `sha256sum t.tar.gz` matches a copy taken before. No `.gezik-*` file or folder is left next to it.

### 16c, commands in the address bar

289. **bash and fish.** With `$SHELL` bash: `>echo $0; ls -la | head -3` shows `bash` and the listing; with `$SHELL` fish (start Gezik from fish, or `SHELL=/usr/bin/fish gezik`): `>echo $FISH_VERSION` shows its version, `>set x 1; echo $x` shows `1`. `>sleep 30` and **Stop**: `pgrep -a sleep` shows none left. `>echo "$GEZIK_FILES"` prints the selected paths.
290. **Programs that want a terminal.** `>sudo ls` ends at once saying a terminal (or a password) is required, and Gezik keeps answering; `>read x; echo done` prints `done`; `>top` ends with an error. `>git log` prints without a pager. The panel's **Copy Output** puts the text on the clipboard (paste it in an editor), **Close** (or Esc on the list) closes the panel.

### 16d, Git status

291. **Changes show by themselves (inotify).** In a repository folder (`git init`, a commit), in a terminal: `echo x >> a.txt` (orange dot on `a.txt` within about 1 s), `git add a.txt` (green dot), `git commit -m t` (gone), `touch new.txt` (grey `?`). Write `build/` into `.gitignore` with a `build` folder present: it turns faint. The status bar shows `⎇ <branch>`. A folder outside any repository and one on an NFS/SMB mount: `pgrep -a git` shows nothing while you browse them. Without git (`[git] program = "/nonexistent"`): "Install Git to see repository status" once, no error box.

### 16e, Git commands

292. **Credentials and errors.** In a clone over HTTPS with the libsecret helper (`git config --global credential.helper /usr/lib/git-core/git-credential-libsecret`, signed in once in a terminal): change, Stage, Commit… (Ctrl+Enter), Push from the status bar's Git menu: the output streams in the panel, exit code 0, `↑1` goes. In a clone of an HTTPS repository with no credentials stored and no helper: Push fails at once (no hang, `pgrep -a git` shows none left), the panel says `terminal prompts disabled` and **Open in Terminal** opens a terminal in the repository with `git push` on the clipboard. Discard Changes… on a changed file and Ctrl+Z brings it back (GNOME and KDE trash).
