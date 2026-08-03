PREFIX ?= /usr/local
DESTDIR ?=
CARGO ?= cargo

.PHONY: build test install install-files uninstall check-demos docs-site check-docs-site

build:
	$(CARGO) build --release --locked

test:
	$(CARGO) test --locked

check-demos:
	python3 scripts/generate_demo_songs.py

docs-site:
	python3 scripts/generate-docs-site.py --write

check-docs-site:
	python3 scripts/generate-docs-site.py --check

install: build install-files

install-files: check-demos
	install -Dm755 target/release/shr $(DESTDIR)$(PREFIX)/bin/shr
	rm -f $(DESTDIR)$(PREFIX)/bin/shsynth
	ln -sfn shr $(DESTDIR)$(PREFIX)/bin/synth-player
	ln -sfn shr $(DESTDIR)$(PREFIX)/bin/shs
	install -Dm755 scripts/setup.sh $(DESTDIR)$(PREFIX)/bin/shr-setup
	install -Dm755 scripts/audio-performance.sh $(DESTDIR)$(PREFIX)/bin/shr-audio-tune
	rm -f $(DESTDIR)$(PREFIX)/bin/shsynth-setup
	install -d $(DESTDIR)$(PREFIX)/share/shsynth/presets/synthv1
	set -e; while IFS= read -r preset; do \
	  install -m644 "presets/synthv1/$$preset" $(DESTDIR)$(PREFIX)/share/shsynth/presets/synthv1/; \
	done < presets/synthv1/cleared-presets.txt
	install -m644 presets/synthv1/cleared-presets.txt $(DESTDIR)$(PREFIX)/share/shsynth/presets/synthv1/
	install -d $(DESTDIR)$(PREFIX)/share/shsynth/config
	install -m644 config/*.conf $(DESTDIR)$(PREFIX)/share/shsynth/config/
	install -d $(DESTDIR)$(PREFIX)/share/shsynth/midi-devices
	install -m644 midi-devices/*.json $(DESTDIR)$(PREFIX)/share/shsynth/midi-devices/
	install -d $(DESTDIR)$(PREFIX)/share/shsynth/controller-profiles
	install -m644 controller-profiles/*.json $(DESTDIR)$(PREFIX)/share/shsynth/controller-profiles/
	install -d $(DESTDIR)$(PREFIX)/share/shsynth/drum-patterns
	install -m644 drum-patterns/*.shdrum $(DESTDIR)$(PREFIX)/share/shsynth/drum-patterns/
	install -m644 drum-patterns/*.shrdrums $(DESTDIR)$(PREFIX)/share/shsynth/drum-patterns/
	install -d $(DESTDIR)$(PREFIX)/share/shsynth/kits
	set -e; while IFS= read -r kit; do \
		case "$$kit" in ''|'#'*) continue ;; */*|.*|*[!a-z0-9.-]*|*.shrkit.*) \
			echo "Unsafe public-kit manifest entry: $$kit" >&2; exit 1 ;; *.shrkit) ;; *) \
			echo "Unsafe public-kit manifest entry: $$kit" >&2; exit 1 ;; esac; \
		source="kits/$$kit"; destination="$(DESTDIR)$(PREFIX)/share/shsynth/kits/$$kit"; \
		[ -d "$$source" ] || { echo "Public kit not found: $$source" >&2; exit 1; }; \
		[ -z "$$(find "$$source" -type l -print -quit)" ] || \
			{ echo "Public kit contains a symlink: $$source" >&2; exit 1; }; \
		install -d "$$destination"; cp -R "$$source/." "$$destination/"; \
	done < kits/cleared-kits.txt
	install -m644 kits/cleared-kits.txt kits/README.md $(DESTDIR)$(PREFIX)/share/shsynth/kits/
	install -d $(DESTDIR)$(PREFIX)/share/shsynth/loops
	set -e; while IFS= read -r loop; do \
	  case "$$loop" in ''|'#'*) continue ;; esac; \
	  install -m644 "loops/$$loop" $(DESTDIR)$(PREFIX)/share/shsynth/loops/; \
	done < loops/cleared-loops.txt
	install -m644 loops/cleared-loops.txt loops/SOURCES.md $(DESTDIR)$(PREFIX)/share/shsynth/loops/
	install -d $(DESTDIR)$(PREFIX)/share/shsynth/demos
	set -e; python3 scripts/generate_demo_songs.py --files | while IFS= read -r demo; do \
	  install -m644 "$$demo" $(DESTDIR)$(PREFIX)/share/shsynth/demos/; \
	done
	install -d $(DESTDIR)$(PREFIX)/share/doc/shsynth/images
	install -m644 LICENSE THIRD_PARTY.md README.md $(DESTDIR)$(PREFIX)/share/doc/shsynth/
	install -m644 docs/*.md $(DESTDIR)$(PREFIX)/share/doc/shsynth/
	install -m644 docs/images/*.html docs/images/*.jpg docs/images/*.png $(DESTDIR)$(PREFIX)/share/doc/shsynth/images/
	install -d $(DESTDIR)$(PREFIX)/share/doc/shsynth/menu
	install -m644 docs/menu/*.md $(DESTDIR)$(PREFIX)/share/doc/shsynth/menu/
	install -d $(DESTDIR)$(PREFIX)/share/doc/shsynth/images/menu
	install -m644 docs/images/menu/*.png $(DESTDIR)$(PREFIX)/share/doc/shsynth/images/menu/

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/shsynth $(DESTDIR)$(PREFIX)/bin/shr
	rm -f $(DESTDIR)$(PREFIX)/bin/synth-player $(DESTDIR)$(PREFIX)/bin/shs
	rm -f $(DESTDIR)$(PREFIX)/bin/shsynth-setup $(DESTDIR)$(PREFIX)/bin/shr-setup
	rm -f $(DESTDIR)$(PREFIX)/bin/shr-audio-tune
	rm -rf $(DESTDIR)$(PREFIX)/share/shsynth
	rm -rf $(DESTDIR)$(PREFIX)/share/doc/shsynth
