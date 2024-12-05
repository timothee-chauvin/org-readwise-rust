# org-readwise-rust

This is a program to sync your [Readwise](https://readwise.io/) collection of articles, books, highlights and notes to plain text files, in this case an [org-roam](https://github.com/org-roam/org-roam) directory.

It's tailored to how I organize my own collection of notes. It's not designed to work for you out of the box, though if your use case is similar, you might only need to make a few changes to the source code.

In addition to storing all your highlights and notes, the source URL of each document is also stored, which pairs well with local archiving solutions like [ArchiveBox](https://github.com/ArchiveBox/ArchiveBox), and local search solutions such as the one described in [this blog post](https://siboehm.com/articles/21/a-local-search-engine).

## Make it run
You need to let the program know about your readwise API key, for instance by adding it in a `.env` file at the top level of this directory (see [.env.template](.env.template)).

This program expects its configuration files in `~/.config/org-readwise-rust/`:
```bash
CONFIG_DIR=~/.config/org-readwise-rust
mkdir -p $CONFIG_DIR
cp config/config.toml $CONFIG_DIR
cp .env $CONFIG_DIR
cp -r templates $CONFIG_DIR
```

There are a few options you can edit in [config.toml](config/config.toml), though you should also read the source code to make sure it does what you want.

Then run `cargo install --path .` to install the executable into `~/.cargo/bin` and use it from anywhere.

## How it works
The program fetches your documents from the [Reader API](https://readwise.io/reader_api), from categories `article`, `epub`, `pdf` (for top-level documents) and `highlight` and `note`.

It also uses [rg](https://github.com/BurntSushi/ripgrep) to find existing references in your current collection, so that files can be edited in place with updated highlights and notes.

References are either:
- for documents that have a source URL: that URL;
- for other documents (e.g. books): the string `@readwise_<readwise_id>` where readwise_id is the "id" field in the API response. The reason it starts with an `@` is so that org-roam considers it as a valid reference (as a side effect, it sees it as a citation key).

If the reference doesn't exist in the collection, a new file is created with a filename based on a slug of the title (duplicate titles are handled by appending a hash of source URL), a new UUID (org-roam needs one), some metadata, then the list of your highlights and notes for this document.

If the reference already exists in the collection, the file is edited. For simplicity, the entire section of highlights and notes is erased (and for even more simplicity, **everything in the file after that section is also nuked**) and re-created from the latest data. Therefore, everything in the file after the beginning of that section should be considered read-only. If you want to make an edit, the readwise link is included with each file, so you should do it there.

This program is designed to be run regularly, e.g. daily. To only update what needs updating, `updatedAfter` is used in the Reader API. However, since we're re-creating the entire highlight and note section whenever we update a document, we only use `updatedAfter` for the top-level documents, and always fetch the full list of highlights and notes (whenever you edit a highlight or note within a document, that document is marked as updated and will show up in the list with `updatedAfter`).

An ideal Reader API would allow us to get all the top-level documents using `updatedAfter`, then get all the highlights and notes within these documents (even those that haven't been updated).

## Sample output
To see what the created files look like, head to the [sample output file](assets/20241203194904-24-theses-on-cybersecurity-and-ai.org) (on github, click on "Raw" to see everything).

## How to run it regularly
You may use any method, but here's a suggestion with `systemctl`:

* `~/.config/systemd/user/org-readwise-rust.service`:
```ini
[Unit]
Description=Run org-readwise-rust

[Service]
ExecStart=/bin/bash -i -c '%h/.cargo/bin/org-readwise-rust'
```

* `~/.config/systemd/user/org-readwise-rust.timer`:
```ini
[Unit]
Description=Run org-readwise-rust
After=network-online.target

[Timer]
OnCalendar=*-*-* 09:00:00
Persistent=true

[Install]
WantedBy=timers.target
```

Then enable the timer:

```bash
systemctl --user enable org-readwise-rust.timer
systemctl --user start org-readwise-rust.timer

# If you want to test:
systemctl --user start org-readwise-rust.service

# To see the logs:
journalctl --user-unit=org-readwise-rust.service
```

## See also
* [org-readwise](https://github.com/CountGreven/org-readwise), written in emacs lisp, has a similar purpose.

## Known issues
* The highlights and notes aren't sorted by their order of appearance in the document, but chronologically based on when you created / updated them.

## FAQ
### Why use `rg` instead of the org-roam SQLite database?
`rg` is just as fast, while being simpler to implement and more reliable.

### The Reader API documentation says that their rate limit is 20 requests per minute, but they only return 100 elements per request. What happens if I have more than 2,000 highlights or notes?
Who knows?

![Who knows?](assets/who_knows.png)

