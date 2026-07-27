#!/usr/bin/env python3
"""Generate the self-contained SHR-DAW documentation microsite."""

from __future__ import annotations

import argparse
import hashlib
import html
import os
import re
import struct
import sys
import tempfile
import tomllib
import unicodedata
from dataclasses import dataclass, field
from pathlib import Path
from urllib.parse import quote, unquote, urlsplit

try:
    import markdown_it
    import mdit_py_plugins
    from markdown_it import MarkdownIt
    from markdown_it.token import Token
    from mdit_py_plugins.tasklists import tasklists_plugin
except ImportError as exc:  # pragma: no cover - exercised by dependency failure
    raise SystemExit(
        "missing documentation renderer; install Debian packages "
        "python3-markdown-it and python3-mdit-py-plugins"
    ) from exc


ROOT = Path(__file__).resolve().parent.parent
DOCS_DIR = ROOT / "docs"
OUTPUT = DOCS_DIR / "index.html"
REPOSITORY_URL = "https://github.com/PaolaShultz/shr-daw"
SITE_URL = "https://paolashultz.github.io/shr-daw/"
SOCIAL_IMAGE = "docs/images/shr-daw-physical-connections.jpg"
EXPECTED_MARKDOWN_IT = "2.1.0"
EXPECTED_MDIT_PLUGINS = "0.3.3"

GROUP_META = {
    "Overview": (
        "Current overview",
        "The concise repository landing page, generated without a second product description.",
    ),
    "Start and use": (
        "Current behaviour",
        "Musician-facing workflows, screens, performance tools, and physical connections.",
    ),
    "Install and configure": (
        "Current behaviour",
        "Installation, first-machine preparation, routing, controllers, and device profiles.",
    ),
    "Architecture and safety": (
        "Current contracts",
        "Implementation boundaries, ownership, recording, routing, and operational safety.",
    ),
    "Measurements and audits": (
        "Dated evidence",
        "Checkpoint measurements and audits. These record evidence, not a second current specification.",
    ),
    "Development record": (
        "Technical archive",
        "Maintainer procedures, build records, handoffs, and historical development evidence.",
    ),
    "Planned work": (
        "Future proposals",
        "Roadmaps and proposals that do not override current behaviour or architecture.",
    ),
    "Technical archive": (
        "Supporting record",
        "Public supporting material not listed as a primary route in the documentation index.",
    ),
    "Licence": (
        "Legal and provenance",
        "Project licence text and the public third-party provenance record.",
    ),
}

GROUP_ORDER = tuple(GROUP_META)
PUBLIC_SCHEMES = {"http", "https", "mailto"}
FORBIDDEN_SCHEMES = {"javascript", "data", "file", "vbscript"}
SECRET_PATTERNS = (
    re.compile(r"\bgh[opurs]_[A-Za-z0-9]{20,}\b"),
    re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
)


@dataclass
class Heading:
    source_slug: str
    generated_id: str
    title: str
    level: int


@dataclass
class Document:
    path: Path
    source: str
    tokens: list[Token]
    title: str = ""
    document_id: str = ""
    group: str = ""
    classification: str = ""
    order: tuple[int, int] = (999, 999)
    headings: list[Heading] = field(default_factory=list)
    fragment_map: dict[str, str] = field(default_factory=dict)
    rendered: str = ""


def fail(message: str) -> "NoReturn":
    raise SystemExit(f"docs site generation failed: {message}")


def check_dependencies() -> None:
    actual = getattr(markdown_it, "__version__", "")
    plugins = getattr(mdit_py_plugins, "__version__", "")
    if actual != EXPECTED_MARKDOWN_IT or plugins != EXPECTED_MDIT_PLUGINS:
        fail(
            "renderer version mismatch: expected markdown-it-py "
            f"{EXPECTED_MARKDOWN_IT} and mdit-py-plugins "
            f"{EXPECTED_MDIT_PLUGINS}, found {actual or 'unknown'} and "
            f"{plugins or 'unknown'}"
        )


def make_markdown() -> MarkdownIt:
    return (
        MarkdownIt(
            "commonmark",
            {
                "html": False,
                "linkify": False,
                "typographer": False,
                "breaks": False,
            },
        )
        .enable(("table", "strikethrough"))
        .use(tasklists_plugin)
    )


def relative_path(path: Path) -> Path:
    try:
        return path.resolve().relative_to(ROOT.resolve())
    except ValueError:
        fail(f"unsafe path outside repository: {path}")


def safe_source(path: Path) -> str:
    if not path.is_file():
        fail(f"missing source: {relative_path(path)}")
    rel = relative_path(path)
    if "user" in rel.parts:
        fail(f"private path is not a public source: {rel}")
    if path.is_symlink():
        fail(f"symlinked source is unsupported: {rel}")
    data = path.read_text(encoding="utf-8")
    for pattern in SECRET_PATTERNS:
        if pattern.search(data):
            fail(f"credential-like content found in public source: {rel}")
    return data


def discover_documents(md: MarkdownIt) -> dict[Path, Document]:
    paths = [ROOT / "README.md"]
    paths.extend(sorted(DOCS_DIR.rglob("*.md")))
    paths.append(ROOT / "THIRD_PARTY.md")
    documents: dict[Path, Document] = {}
    for path in paths:
        rel = relative_path(path)
        if rel.suffix.lower() != ".md":
            fail(f"unsupported documentation input: {rel}")
        if rel in documents:
            fail(f"duplicate documentation source: {rel}")
        source = safe_source(path)
        documents[rel] = Document(path=rel, source=source, tokens=md.parse(source))
    return documents


def inline_text(token: Token) -> str:
    if not token.children:
        return token.content.strip()
    parts: list[str] = []
    for child in token.children:
        if child.type in {"text", "code_inline", "image"}:
            parts.append(child.content)
        elif child.type in {"softbreak", "hardbreak"}:
            parts.append(" ")
    return " ".join("".join(parts).split())


def github_slug(text: str) -> str:
    normalized = unicodedata.normalize("NFKC", text).casefold()
    kept = "".join(
        char
        for char in normalized
        if char.isalnum() or char in {" ", "-", "_"} or char.isspace()
    )
    return "-".join(kept.split())


def doc_id(path: Path) -> str:
    if path == Path("README.md"):
        return "doc-readme"
    if path == Path("docs/README.md"):
        return "doc-documentation-index"
    if path == Path("THIRD_PARTY.md"):
        return "doc-third-party"
    stem = path.with_suffix("").as_posix()
    if stem.startswith("docs/"):
        stem = stem[5:]
    return "doc-" + github_slug(stem.replace("/", " "))


def assign_headings(documents: dict[Path, Document]) -> set[str]:
    anchors: set[str] = {"top", "main-content", "start-here", "screenshots"}
    for doc in documents.values():
        doc.document_id = doc_id(doc.path)
        if doc.document_id in anchors:
            fail(f"duplicate generated anchor: {doc.document_id}")
        anchors.add(doc.document_id)
        slug_counts: dict[str, int] = {}
        found_heading = False
        found_title = False
        for index, token in enumerate(doc.tokens):
            if token.type != "heading_open":
                continue
            if index + 1 >= len(doc.tokens) or doc.tokens[index + 1].type != "inline":
                fail(f"unsupported heading structure in {doc.path}")
            inline = doc.tokens[index + 1]
            title = inline_text(inline)
            if not title:
                fail(f"empty heading in {doc.path}")
            base_slug = github_slug(title)
            if not base_slug:
                fail(f"heading has no usable anchor in {doc.path}: {title!r}")
            occurrence = slug_counts.get(base_slug, 0)
            slug_counts[base_slug] = occurrence + 1
            source_slug = base_slug if occurrence == 0 else f"{base_slug}-{occurrence}"
            original_level = int(token.tag[1])
            generated = (
                doc.document_id
                if not found_title and original_level == 1
                else f"{doc.document_id}--{source_slug}"
            )
            if generated != doc.document_id and generated in anchors:
                fail(f"duplicate generated anchor: {generated}")
            anchors.add(generated)
            display_level = min(6, original_level + 2)
            token.tag = f"h{display_level}"
            token.attrSet("id", generated)
            token.attrSet("tabindex", "-1")
            closing = doc.tokens[index + 2] if index + 2 < len(doc.tokens) else None
            if not closing or closing.type != "heading_close":
                fail(f"unsupported heading close in {doc.path}: {title}")
            closing.tag = token.tag
            anchor = Token("html_inline", "", 0)
            anchor.content = (
                f'<a class="heading-anchor" href="#{generated}" '
                f'aria-label="Link to {html.escape(title, quote=True)}">#</a>'
            )
            if inline.children is None:
                inline.children = []
            inline.children.append(anchor)
            doc.headings.append(
                Heading(
                    source_slug=source_slug,
                    generated_id=generated,
                    title=title,
                    level=display_level,
                )
            )
            doc.fragment_map[source_slug] = generated
            if not found_title and original_level == 1:
                doc.title = title
                found_title = True
            found_heading = True
        if not found_heading:
            fail(f"missing top-level heading in {doc.path}")
        if not found_title:
            if doc.path != Path("README.md"):
                fail(f"missing level-one title in {doc.path}")
            doc.title = "SHR-DAW"
            synthetic = (
                '<h3 id="doc-readme" tabindex="-1">SHR-DAW'
                '<a class="heading-anchor" href="#doc-readme" '
                'aria-label="Link to SHR-DAW">#</a></h3>\n'
            )
            doc.rendered = synthetic
    return anchors


def resolve_path(source: Path, raw_path: str) -> Path:
    if not raw_path:
        return source
    decoded = unquote(raw_path)
    if "\x00" in decoded or "\\" in decoded:
        fail(f"unsafe local path in {source}: {raw_path}")
    candidate = (ROOT / source).parent.joinpath(decoded).resolve()
    rel = relative_path(candidate)
    if "user" in rel.parts:
        fail(f"link from {source} enters private user storage: {raw_path}")
    if not candidate.exists():
        fail(f"broken local link in {source}: {raw_path}")
    return rel


def append_class(token: Token, class_name: str) -> None:
    existing = token.attrGet("class")
    token.attrSet("class", f"{existing} {class_name}".strip() if existing else class_name)


def source_url(path: Path, fragment: str = "") -> str:
    kind = "tree" if (ROOT / path).is_dir() else "blob"
    url = f"{REPOSITORY_URL}/{kind}/main/{quote(path.as_posix(), safe='/')}"
    if fragment:
        url += "#" + quote(unquote(fragment), safe="-_")
    return url


def image_size(path: Path) -> tuple[int, int]:
    data = path.read_bytes()
    if data.startswith(b"\x89PNG\r\n\x1a\n") and len(data) >= 24:
        return struct.unpack(">II", data[16:24])
    if data.startswith(b"\xff\xd8"):
        offset = 2
        sof_markers = {
            0xC0,
            0xC1,
            0xC2,
            0xC3,
            0xC5,
            0xC6,
            0xC7,
            0xC9,
            0xCA,
            0xCB,
            0xCD,
            0xCE,
            0xCF,
        }
        while offset + 4 <= len(data):
            if data[offset] != 0xFF:
                offset += 1
                continue
            marker = data[offset + 1]
            offset += 2
            if marker in {0xD8, 0xD9}:
                continue
            if offset + 2 > len(data):
                break
            segment_length = struct.unpack(">H", data[offset : offset + 2])[0]
            if segment_length < 2 or offset + segment_length > len(data):
                break
            if marker in sof_markers and segment_length >= 7:
                height, width = struct.unpack(">HH", data[offset + 3 : offset + 7])
                return width, height
            offset += segment_length
    fail(f"unsupported image format: {relative_path(path)}")


def rewrite_url(
    doc: Document,
    raw: str,
    documents: dict[Path, Document],
    *,
    image: bool,
) -> tuple[str, bool, Path | None]:
    parsed = urlsplit(raw)
    scheme = parsed.scheme.casefold()
    if scheme in FORBIDDEN_SCHEMES:
        fail(f"unsafe URL scheme in {doc.path}: {raw}")
    if scheme or parsed.netloc:
        if image:
            fail(f"remote image is unsupported in {doc.path}: {raw}")
        if scheme not in PUBLIC_SCHEMES:
            fail(f"unsupported URL scheme in {doc.path}: {raw}")
        return raw, True, None
    if parsed.query:
        fail(f"query strings on local links are unsupported in {doc.path}: {raw}")
    if parsed.path.startswith("/"):
        fail(f"root-relative local link is unsafe in {doc.path}: {raw}")
    target = resolve_path(doc.path, parsed.path)
    if image:
        image_root = Path("docs/images")
        if target != image_root and image_root not in target.parents:
            fail(f"image outside docs/images in {doc.path}: {raw}")
        if not (ROOT / target).is_file():
            fail(f"missing image in {doc.path}: {raw}")
        return (target.relative_to("docs").as_posix(), False, target)
    if target == Path("LICENSE"):
        if parsed.fragment:
            fail(f"LICENSE link cannot contain a fragment in {doc.path}: {raw}")
        return "#doc-license", False, None
    if target in documents:
        destination = documents[target]
        if parsed.fragment:
            fragment = unquote(parsed.fragment)
            generated = destination.fragment_map.get(fragment)
            if generated is None:
                fail(
                    f"broken heading fragment in {doc.path}: {raw} "
                    f"(target {target})"
                )
            return f"#{generated}", False, None
        return f"#{destination.document_id}", False, None
    return source_url(target, parsed.fragment), True, None


def style_admonitions(tokens: list[Token]) -> None:
    for index, token in enumerate(tokens):
        if token.type != "blockquote_open":
            continue
        for candidate in tokens[index + 1 :]:
            if candidate.type == "blockquote_close":
                break
            if candidate.type != "inline" or not candidate.children:
                continue
            first = candidate.children[0]
            if first.type == "text" and first.content in {
                "[!WARNING]",
                "[!NOTE]",
                "[!IMPORTANT]",
                "[!CAUTION]",
                "[!TIP]",
            }:
                kind = first.content[2:-1].casefold()
                append_class(token, f"admonition {kind}")
                first.content = first.content[2:-1].title()
                first.tag = "strong"
                first.type = "text"
            break


def rewrite_tokens(
    documents: dict[Path, Document],
    md: MarkdownIt,
) -> set[Path]:
    referenced_images: set[Path] = set()
    for doc in documents.values():
        style_admonitions(doc.tokens)
        for token in doc.tokens:
            children = token.children or []
            for child in children:
                if child.type == "link_open":
                    href = child.attrGet("href")
                    if href is None:
                        fail(f"link without destination in {doc.path}")
                    rewritten, external, _ = rewrite_url(
                        doc, href, documents, image=False
                    )
                    child.attrSet("href", rewritten)
                    if external:
                        append_class(child, "external")
                        child.attrSet("target", "_blank")
                        child.attrSet("rel", "noopener noreferrer external")
                elif child.type == "image":
                    src = child.attrGet("src")
                    if src is None:
                        fail(f"image without source in {doc.path}")
                    rewritten, _, image_path = rewrite_url(
                        doc, src, documents, image=True
                    )
                    assert image_path is not None
                    width, height = image_size(ROOT / image_path)
                    digest = hashlib.sha256((ROOT / image_path).read_bytes()).hexdigest()
                    child.attrSet("src", rewritten)
                    child.attrSet("width", str(width))
                    child.attrSet("height", str(height))
                    child.attrSet("loading", "lazy")
                    child.attrSet("decoding", "async")
                    child.attrSet("data-source-sha256", digest)
                    if image_path.suffix.casefold() == ".png":
                        append_class(child, "tui-shot")
                    referenced_images.add(image_path)
        doc.rendered += md.renderer.render(doc.tokens, md.options, {})
    return referenced_images


def documentation_groups(
    documents: dict[Path, Document],
    md: MarkdownIt,
) -> dict[str, list[Document]]:
    index_path = Path("docs/README.md")
    index = documents.get(index_path)
    if index is None:
        fail("missing source: docs/README.md")
    # Parse a fresh tree because the display tokens already have rewritten links.
    tokens = md.parse(index.source)
    current_group = ""
    mapped: dict[Path, tuple[str, int]] = {}
    group_positions = {name: 0 for name in GROUP_ORDER}
    found_groups: set[str] = set()
    for token_index, token in enumerate(tokens):
        if token.type == "heading_open" and token.tag == "h2":
            if token_index + 1 >= len(tokens):
                fail("malformed heading in docs/README.md")
            title = inline_text(tokens[token_index + 1])
            current_group = title if title in GROUP_META else ""
            if current_group:
                found_groups.add(current_group)
            continue
        if token.type != "inline" or not current_group:
            continue
        for child in token.children or []:
            if child.type != "link_open":
                continue
            href = child.attrGet("href") or ""
            parsed = urlsplit(href)
            if parsed.scheme or parsed.netloc or not parsed.path:
                continue
            target = resolve_path(index_path, parsed.path)
            if target not in documents:
                continue
            if target in mapped and mapped[target][0] != current_group:
                fail(f"documentation index assigns {target} to multiple groups")
            position = group_positions[current_group]
            mapped[target] = (current_group, position)
            group_positions[current_group] = position + 1
    required = {
        "Start and use",
        "Install and configure",
        "Architecture and safety",
        "Measurements and audits",
        "Development record",
        "Planned work",
    }
    missing_groups = required - found_groups
    if missing_groups:
        fail(
            "docs/README.md is missing required groups: "
            + ", ".join(sorted(missing_groups))
        )

    documents[Path("README.md")].group = "Overview"
    documents[Path("README.md")].order = (0, 0)
    index.group = "Start and use"
    index.order = (0, 0)
    for path, (group, position) in mapped.items():
        doc = documents[path]
        doc.group = group
        doc.order = (1, position)

    manual = documents.get(Path("docs/MENU_MANUAL.md"))
    manual_position = manual.order[1] if manual and manual.group else 999
    for position, path in enumerate(sorted(documents)):
        doc = documents[path]
        if doc.group:
            continue
        if path.parts[:2] == ("docs", "menu"):
            doc.group = "Start and use"
            doc.order = (2, manual_position * 10 + position)
        elif path == Path("THIRD_PARTY.md"):
            doc.group = "Licence"
            doc.order = (0, 0)
        else:
            doc.group = "Technical archive"
            doc.order = (1, position)

    licence_source = safe_source(ROOT / "LICENSE")
    licence_doc = Document(
        path=Path("LICENSE"),
        source=licence_source,
        tokens=[],
        title="MIT licence",
        document_id="doc-license",
        group="Licence",
        classification=GROUP_META["Licence"][0],
        order=(1, 0),
        rendered=(
            '<h3 id="doc-license" tabindex="-1">MIT licence'
            '<a class="heading-anchor" href="#doc-license" '
            'aria-label="Link to MIT licence">#</a></h3>\n'
            f"<pre class=\"licence-text\"><code>{html.escape(licence_source)}</code></pre>\n"
        ),
    )

    groups = {name: [] for name in GROUP_ORDER}
    for doc in documents.values():
        doc.classification = GROUP_META[doc.group][0]
        groups[doc.group].append(doc)
    groups["Licence"].append(licence_doc)
    for docs in groups.values():
        docs.sort(key=lambda item: (item.order, item.path.as_posix()))
    return groups


def find_intro(doc: Document) -> str:
    for index, token in enumerate(doc.tokens):
        if token.type != "inline" or index == 0:
            continue
        if doc.tokens[index - 1].type != "paragraph_open":
            continue
        if any(child.type == "image" for child in token.children or []):
            continue
        text = inline_text(token)
        if text and not text.startswith("[!"):
            return text
    fail("README.md has no introductory paragraph")


def find_features(doc: Document) -> list[str]:
    in_features = False
    features: list[str] = []
    item_depth = 0
    for index, token in enumerate(doc.tokens):
        if token.type == "heading_open":
            title = inline_text(doc.tokens[index + 1])
            if title == "Features":
                in_features = True
                continue
            if in_features:
                break
        if not in_features:
            continue
        if token.type == "list_item_open":
            item_depth += 1
        elif token.type == "list_item_close":
            item_depth = max(0, item_depth - 1)
        elif token.type == "inline" and item_depth == 1:
            text = inline_text(token)
            if text:
                features.append(text)
    if not features:
        fail("README.md has no feature list")
    return features


def find_gallery(doc: Document) -> list[tuple[str, Token]]:
    in_gallery = False
    title = ""
    gallery: list[tuple[str, Token]] = []
    for index, token in enumerate(doc.tokens):
        if token.type == "heading_open":
            heading = inline_text(doc.tokens[index + 1])
            original_level = max(1, int(token.tag[1]) - 2)
            if original_level == 2 and heading == "Screenshot tour":
                in_gallery = True
                continue
            if in_gallery and original_level == 2:
                break
            if in_gallery and original_level == 3:
                title = heading
        if not in_gallery or token.type != "inline":
            continue
        for child in token.children or []:
            if child.type == "image":
                gallery.append((title or child.content, child))
    if not gallery:
        fail("README.md has no screenshot tour")
    return gallery


def image_html(token: Token, *, eager: bool = False) -> str:
    attrs = dict(token.attrs or {})
    attrs["alt"] = token.content
    if eager:
        attrs["loading"] = "eager"
        attrs["fetchpriority"] = "high"
    else:
        attrs.setdefault("loading", "lazy")
    return "<img " + " ".join(
        f'{html.escape(str(key), quote=True)}="{html.escape(str(value), quote=True)}"'
        for key, value in sorted(attrs.items())
    ) + ">"


def nav_html(groups: dict[str, list[Document]]) -> str:
    sections: list[str] = []
    for group in GROUP_ORDER:
        docs = groups[group]
        if not docs:
            continue
        links = "".join(
            f'<li><a href="#{doc.document_id}">{html.escape(doc.title)}</a></li>'
            for doc in docs
        )
        open_attr = " open" if group in {"Start and use", "Overview"} else ""
        sections.append(
            f"<details{open_attr}><summary>{html.escape(group)}</summary>"
            f"<ul>{links}</ul></details>"
        )
    return "\n".join(sections)


def source_link(path: Path) -> str:
    return source_url(path)


def content_html(groups: dict[str, list[Document]]) -> str:
    output: list[str] = []
    for group in GROUP_ORDER:
        docs = groups[group]
        if not docs:
            continue
        classification, description = GROUP_META[group]
        group_id = "group-" + github_slug(group)
        output.append(
            f'<section class="doc-group" aria-labelledby="{group_id}">'
            f'<header class="group-header"><p class="eyebrow">{html.escape(classification)}</p>'
            f'<h2 id="{group_id}">{html.escape(group)}</h2>'
            f"<p>{html.escape(description)}</p></header>"
        )
        for doc in docs:
            rel = doc.path.as_posix()
            source = (
                f'<a class="external source-link" href="{source_link(doc.path)}" '
                'target="_blank" rel="noopener noreferrer external">'
                f"View {html.escape(rel)} source</a>"
            )
            output.append(
                f'<article class="document" data-source="{html.escape(rel, quote=True)}" '
                f'data-kind="{html.escape(classification, quote=True)}">'
                f'<div class="document-meta"><span>{html.escape(classification)}</span>'
                f"{source}</div>{doc.rendered}"
                '<p class="back-top"><a href="#top">Back to top</a></p></article>'
            )
        output.append("</section>")
    return "\n".join(output)


CSS = r"""
:root {
  color-scheme: dark;
  --bg: #090c0b;
  --panel: #111614;
  --panel-2: #171d1a;
  --line: #35413b;
  --text: #eef3ef;
  --muted: #aab6af;
  --green: #72d28c;
  --green-bright: #a0f2b1;
  --yellow: #f1d06b;
  --red: #f07373;
  --max: 78rem;
  --nav: 18rem;
}
* { box-sizing: border-box; }
html { scroll-padding-top: 5rem; }
body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont,
    "Segoe UI", sans-serif;
  font-size: 1rem;
  line-height: 1.65;
}
body::before {
  content: "";
  position: fixed;
  inset: 0 0 auto;
  height: .2rem;
  background: linear-gradient(90deg, var(--green) 0 72%, var(--yellow) 72% 90%, var(--red) 90%);
  z-index: 100;
}
a { color: var(--green-bright); text-underline-offset: .18em; }
a:hover { color: #fff; }
a.external::after { content: " ↗"; color: var(--yellow); font-size: .8em; }
:focus-visible { outline: .2rem solid var(--yellow); outline-offset: .18rem; }
.skip-link {
  position: fixed; top: .5rem; left: .5rem; z-index: 200;
  padding: .6rem .8rem; background: var(--yellow); color: #111;
  transform: translateY(-160%);
}
.skip-link:focus { transform: translateY(0); }
.sr-only {
  position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px;
  overflow: hidden; clip: rect(0,0,0,0); white-space: nowrap; border: 0;
}
.site-header {
  position: sticky; top: 0; z-index: 50;
  display: flex; align-items: center; justify-content: space-between; gap: 1rem;
  min-height: 4rem; padding: .65rem max(1rem, calc((100vw - var(--max))/2));
  border-bottom: 1px solid var(--line); background: rgba(9,12,11,.96);
}
.brand { display: flex; align-items: center; gap: .65rem; color: var(--text); text-decoration: none; font-weight: 800; }
.leds { display: inline-flex; gap: .28rem; }
.led { width: .55rem; height: .55rem; border-radius: 50%; display: inline-block; background: var(--green); box-shadow: 0 0 .5rem rgba(114,210,140,.45); }
.led.yellow { background: var(--yellow); box-shadow: none; }
.led.red { background: var(--red); box-shadow: none; }
.header-links { display: flex; align-items: center; gap: 1rem; font-size: .9rem; }
.nav-toggle {
  display: none; border: 1px solid var(--line); border-radius: .35rem;
  background: var(--panel); color: var(--text); padding: .5rem .7rem; font: inherit;
}
.site-shell {
  display: grid; grid-template-columns: var(--nav) minmax(0, 1fr); gap: 2.2rem;
  width: min(var(--max), calc(100% - 2rem)); margin: 0 auto;
}
.site-nav {
  position: sticky; top: 5rem; align-self: start; max-height: calc(100vh - 6rem);
  overflow-y: auto; padding: 1.25rem 0 2rem; scrollbar-width: thin;
}
.site-nav h2 { font-size: .8rem; text-transform: uppercase; letter-spacing: .12em; color: var(--muted); }
.search-box { margin-bottom: 1rem; }
.search-box label { display: block; margin-bottom: .35rem; font-size: .85rem; color: var(--muted); }
.search-box input {
  width: 100%; border: 1px solid var(--line); border-radius: .35rem;
  background: var(--panel); color: var(--text); padding: .62rem .7rem; font: inherit;
}
.search-help { margin: .35rem 0 0; color: var(--muted); font-size: .75rem; }
#search-results { margin: .7rem 0 1.1rem; padding: 0; list-style: none; }
#search-results li { margin-bottom: .65rem; }
#search-results a { display: block; font-weight: 700; font-size: .9rem; }
#search-results small { color: var(--muted); display: block; line-height: 1.35; }
.site-nav details { border-top: 1px solid var(--line); padding: .55rem 0; }
.site-nav summary { cursor: pointer; color: var(--text); font-weight: 750; font-size: .88rem; }
.site-nav ul { margin: .55rem 0 0; padding: 0 0 0 .8rem; list-style: none; }
.site-nav li { margin: .28rem 0; line-height: 1.3; }
.site-nav li a { color: var(--muted); font-size: .82rem; text-decoration: none; }
.site-nav li a:hover, .site-nav li a[aria-current="location"] { color: var(--green-bright); }
main { min-width: 0; padding-bottom: 5rem; }
.hero { padding: 2.2rem 0 3rem; border-bottom: 1px solid var(--line); }
.hero-copy { max-width: 52rem; }
.eyebrow {
  margin: 0 0 .35rem; color: var(--green); font-size: .78rem; font-weight: 800;
  letter-spacing: .13em; text-transform: uppercase;
}
.hero h1 { margin: 0; font-size: clamp(2.25rem, 8vw, 5.25rem); line-height: .95; letter-spacing: -.055em; }
.hero .intro { max-width: 48rem; color: #d7dfda; font-size: clamp(1.03rem, 2vw, 1.25rem); }
.status-line { display: flex; flex-wrap: wrap; gap: .65rem; align-items: center; color: var(--muted); font-size: .85rem; }
.version { padding: .25rem .55rem; border: 1px solid var(--green); border-radius: 999px; color: var(--green-bright); }
.hero-image {
  display: block; width: 100%; height: auto; margin-top: 1.6rem;
  border: 1px solid var(--line); border-radius: .45rem; background: #000;
}
.button-row { display: flex; flex-wrap: wrap; gap: .65rem; margin-top: 1.25rem; }
.button {
  display: inline-flex; align-items: center; min-height: 2.7rem; padding: .55rem .85rem;
  border: 1px solid var(--green); border-radius: .35rem; color: var(--text);
  font-weight: 750; text-decoration: none; background: #102218;
}
.button.secondary { border-color: var(--line); background: var(--panel); }
.overview-block { padding: 2.5rem 0; border-bottom: 1px solid var(--line); }
.overview-block h2 { margin-top: 0; font-size: clamp(1.5rem, 4vw, 2.25rem); }
.feature-grid, .start-grid {
  display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: .8rem;
}
.feature-card, .start-card {
  min-width: 0; padding: 1rem; border: 1px solid var(--line); border-radius: .4rem;
  background: var(--panel);
}
.feature-card { position: relative; padding-left: 2.1rem; }
.feature-card::before {
  content: ""; position: absolute; left: .9rem; top: 1.45rem; width: .55rem; height: .55rem;
  border-radius: 50%; background: var(--green);
}
.start-card { color: var(--text); text-decoration: none; }
.start-card strong { display: block; color: var(--green-bright); }
.start-card span { color: var(--muted); font-size: .9rem; }
.screenshot-grid {
  display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 1rem;
}
.screenshot-grid figure { margin: 0; min-width: 0; }
.screenshot-grid img {
  display: block; width: 100%; height: auto; border: 1px solid var(--line);
  border-radius: .3rem; background: #000;
}
.screenshot-grid figcaption { padding-top: .4rem; color: var(--muted); font-size: .85rem; }
.doc-group { padding: 3rem 0 0; }
.group-header { max-width: 48rem; margin-bottom: 1.5rem; }
.group-header h2 { margin: 0; font-size: clamp(1.8rem, 5vw, 3rem); line-height: 1.05; }
.group-header > p:last-child { color: var(--muted); }
.document {
  min-width: 0; margin: 0 0 2rem; padding: clamp(1rem, 3vw, 2rem);
  border: 1px solid var(--line); border-radius: .45rem; background: var(--panel);
  overflow-wrap: anywhere;
}
.document-meta {
  display: flex; flex-wrap: wrap; justify-content: space-between; gap: .5rem 1rem;
  margin-bottom: .8rem; color: var(--muted); font-size: .78rem; text-transform: uppercase;
  letter-spacing: .08em;
}
.document-meta .source-link { text-transform: none; letter-spacing: normal; }
.document h3 { margin: .2rem 0 1rem; font-size: clamp(1.45rem, 4vw, 2.25rem); line-height: 1.15; }
.document h4 { margin: 2.2rem 0 .65rem; font-size: clamp(1.2rem, 3vw, 1.6rem); line-height: 1.25; }
.document h5 { margin: 1.8rem 0 .55rem; font-size: 1.13rem; }
.document h6 { margin: 1.6rem 0 .5rem; font-size: 1rem; color: var(--yellow); }
.heading-anchor {
  margin-left: .4rem; color: var(--muted); font-size: .65em; opacity: 0;
  text-decoration: none;
}
h3:hover .heading-anchor, h4:hover .heading-anchor, h5:hover .heading-anchor,
h6:hover .heading-anchor, .heading-anchor:focus { opacity: 1; }
p, li { max-width: 76ch; }
blockquote {
  margin: 1.2rem 0; padding: .55rem 1rem; border-left: .25rem solid var(--green);
  background: var(--panel-2); color: #dce5df;
}
blockquote.warning, blockquote.caution { border-color: var(--red); }
blockquote.important { border-color: var(--yellow); }
code, kbd {
  font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
  font-size: .9em;
}
:not(pre) > code { padding: .1em .3em; border-radius: .2rem; background: #050706; color: #d8f7df; }
pre {
  max-width: 100%; overflow-x: auto; padding: 1rem; border: 1px solid #2b352f;
  border-radius: .35rem; background: #050706; color: #e7eee9; tab-size: 4;
  white-space: pre;
}
table {
  display: block; width: max-content; max-width: 100%; overflow-x: auto;
  border-collapse: collapse; margin: 1.1rem 0;
}
th, td { padding: .5rem .65rem; border: 1px solid var(--line); text-align: left; vertical-align: top; }
th { background: #1b241f; color: var(--green-bright); }
.document img { display: block; max-width: 100%; height: auto; margin: 1rem auto; }
.tui-shot { image-rendering: crisp-edges; image-rendering: pixelated; background: #000; }
.contains-task-list { padding-left: .4rem; list-style: none; }
.task-list-item-checkbox { margin-right: .45rem; accent-color: var(--green); }
.licence-text { white-space: pre-wrap; }
.back-top { margin-top: 2rem; font-size: .8rem; }
.page-footer {
  margin-top: 4rem; padding: 2rem 0; border-top: 1px solid var(--line);
  color: var(--muted); font-size: .88rem;
}
noscript p { padding: .7rem; border: 1px solid var(--yellow); color: var(--yellow); }
@media (max-width: 860px) {
  .site-header { padding-inline: 1rem; }
  .header-links > a { display: none; }
  .js .nav-toggle { display: inline-block; }
  .site-shell { display: block; width: min(100% - 1.2rem, 52rem); }
  .site-nav {
    position: static; max-height: none; padding: 1rem 0;
  }
  .js .site-nav {
    position: fixed; inset: 4rem auto 0 0; z-index: 60; width: min(88vw, 21rem);
    max-height: none; padding: 1rem; border-right: 1px solid var(--line);
    background: #0c100e; transform: translateX(-105%); transition: transform .18s ease;
  }
  .js body.nav-open { overflow: hidden; }
  .js body.nav-open .site-nav { transform: translateX(0); box-shadow: 1rem 0 3rem rgba(0,0,0,.45); }
  main { padding-top: 0; }
}
@media (max-width: 560px) {
  body { font-size: .96rem; }
  .site-shell { width: min(100% - .8rem, 52rem); }
  .hero { padding-top: 1.5rem; }
  .feature-grid, .start-grid, .screenshot-grid { grid-template-columns: 1fr; }
  .document { padding: 1rem .75rem; }
  .document-meta { display: block; }
  .document-meta > * { display: block; margin-bottom: .3rem; }
  ol, ul { padding-left: 1.35rem; }
}
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after { scroll-behavior: auto !important; transition: none !important; animation: none !important; }
}
@media print {
  :root { color-scheme: light; --text: #111; --muted: #444; --line: #aaa; --panel: #fff; --panel-2: #f4f4f4; }
  body { background: #fff; color: #111; font-size: 10pt; }
  body::before, .site-header, .site-nav, .button-row, .back-top, .heading-anchor, .page-footer { display: none !important; }
  .site-shell { display: block; width: auto; margin: 0; }
  .hero { padding-top: 0; }
  .hero-image { max-height: 4in; object-fit: contain; }
  .document { border: 0; padding: 0; break-inside: auto; }
  .document h3, .document h4 { break-after: avoid; }
  pre, table, blockquote, figure { break-inside: avoid; }
  a { color: #111; text-decoration: underline; }
  a.external::after { content: " (" attr(href) ")"; font-size: .75em; overflow-wrap: anywhere; }
  .doc-group { break-before: page; }
  .doc-group:first-of-type { break-before: auto; }
}
"""


JS = r"""
(() => {
  "use strict";
  const body = document.body;
  const toggle = document.querySelector(".nav-toggle");
  const nav = document.getElementById("site-nav");
  const search = document.getElementById("doc-search");
  const results = document.getElementById("search-results");
  const status = document.getElementById("search-status");

  const setNav = (open) => {
    body.classList.toggle("nav-open", open);
    toggle.setAttribute("aria-expanded", String(open));
    if (open) {
      const first = nav.querySelector("input, a, summary");
      if (first) first.focus();
    } else {
      toggle.focus();
    }
  };
  toggle.addEventListener("click", () => setNav(!body.classList.contains("nav-open")));
  nav.addEventListener("click", (event) => {
    if (event.target.closest("a") && matchMedia("(max-width: 860px)").matches) {
      body.classList.remove("nav-open");
      toggle.setAttribute("aria-expanded", "false");
    }
  });

  const normalize = (value) => value.normalize("NFKD").toLocaleLowerCase();
  let entries;
  const buildSearchIndex = () => {
    if (entries) return entries;
    const articles = [...document.querySelectorAll("main article")];
    const articleEntries = articles.map((article) => {
      const heading = article.querySelector("h3[id]");
      return {
        title: heading.textContent.replace(/#$/, "").trim(),
        href: `#${heading.id}`,
        source: article.dataset.source,
        kind: article.dataset.kind,
        text: normalize(article.textContent)
      };
    });
    const headingEntries = [...document.querySelectorAll("main article h4, main article h5, main article h6")].map((heading) => {
      const article = heading.closest("article");
      const title = heading.textContent.replace(/#$/, "").trim();
      return {
        title,
        href: `#${heading.id}`,
        source: article.dataset.source,
        kind: article.dataset.kind,
        text: normalize(title)
      };
    });
    entries = [...headingEntries, ...articleEntries];
    return entries;
  };

  const renderSearch = () => {
    const query = normalize(search.value.trim());
    results.replaceChildren();
    if (query.length < 2) {
      status.textContent = query ? "Type at least two characters." : "Search all headings and document text.";
      return;
    }
    const terms = query.split(/\s+/).filter(Boolean);
    const matches = buildSearchIndex()
      .filter((entry) => terms.every((term) => entry.text.includes(term)))
      .slice(0, 30);
    for (const entry of matches) {
      const item = document.createElement("li");
      const link = document.createElement("a");
      const meta = document.createElement("small");
      link.href = entry.href;
      link.textContent = entry.title;
      meta.textContent = `${entry.kind} · ${entry.source}`;
      item.append(link, meta);
      results.append(item);
    }
    status.textContent = matches.length
      ? `${matches.length} result${matches.length === 1 ? "" : "s"} shown.`
      : "No matching documentation.";
  };
  search.addEventListener("input", renderSearch);

  document.addEventListener("keydown", (event) => {
    const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement.tagName);
    if (event.key === "/" && !typing) {
      event.preventDefault();
      search.focus();
      if (matchMedia("(max-width: 860px)").matches && !body.classList.contains("nav-open")) {
        body.classList.add("nav-open");
        toggle.setAttribute("aria-expanded", "true");
      }
    } else if (event.key === "Escape" && body.classList.contains("nav-open")) {
      setNav(false);
    }
  });

  if ("IntersectionObserver" in window) {
    const links = new Map([...nav.querySelectorAll('a[href^="#doc-"]')].map((link) => [link.hash.slice(1), link]));
    const observer = new IntersectionObserver((items) => {
      for (const item of items) {
        if (!item.isIntersecting) continue;
        for (const link of links.values()) link.removeAttribute("aria-current");
        const link = links.get(item.target.id);
        if (link) link.setAttribute("aria-current", "location");
      }
    }, { rootMargin: "-15% 0px -75% 0px" });
    for (const heading of document.querySelectorAll("article > h3[id]")) observer.observe(heading);
  }
})();
"""


def build_page(
    documents: dict[Path, Document],
    groups: dict[str, list[Document]],
    version: str,
) -> str:
    readme = documents[Path("README.md")]
    intro = find_intro(readme)
    features = find_features(readme)
    gallery = find_gallery(readme)
    first_image = next(
        (
            child
            for token in readme.tokens
            if token.type == "inline"
            for child in (token.children or [])
            if child.type == "image"
        ),
        None,
    )
    if first_image is None:
        fail("README.md has no header image")

    feature_cards = "".join(
        f'<div class="feature-card">{html.escape(feature)}</div>'
        for feature in features
    )
    screenshot_cards = "".join(
        f"<figure>{image_html(image)}<figcaption>{html.escape(title)}</figcaption></figure>"
        for title, image in gallery[:6]
    )
    start_docs = [
        documents[Path("docs/FIRST_RUN.md")],
        documents[Path("docs/USING_SHR_DAW.md")],
        documents[Path("docs/INSTALLATION.md")],
        documents[Path("docs/CONFIGURATION.md")],
    ]
    start_cards = "".join(
        f'<a class="start-card" href="#{doc.document_id}">'
        f"<strong>{html.escape(doc.title)}</strong>"
        f"<span>{html.escape(doc.classification)}</span></a>"
        for doc in start_docs
    )
    social_path = ROOT / SOCIAL_IMAGE
    if not social_path.is_file():
        fail(f"missing social preview image: {SOCIAL_IMAGE}")
    social_width, social_height = image_size(social_path)
    if (social_width, social_height) != (1200, 675):
        fail(
            "social preview image must remain 1200x675, found "
            f"{social_width}x{social_height}"
        )

    parts = [
        "<!doctype html>\n",
        '<html lang="en" class="no-js">\n<head>\n',
        '<meta charset="utf-8">\n',
        '<meta name="viewport" content="width=device-width, initial-scale=1">\n',
        '<meta name="color-scheme" content="dark">\n',
        '<meta name="theme-color" content="#090c0b">\n',
        "<title>SHR-DAW documentation</title>\n",
        f'<meta name="description" content="{html.escape(intro, quote=True)}">\n',
        f'<link rel="canonical" href="{SITE_URL}">\n',
        '<meta property="og:type" content="website">\n',
        '<meta property="og:site_name" content="SHR-DAW">\n',
        '<meta property="og:title" content="SHR-DAW — Raspberry Pi mini DAW">\n',
        f'<meta property="og:description" content="{html.escape(intro, quote=True)}">\n',
        f'<meta property="og:url" content="{SITE_URL}">\n',
        f'<meta property="og:image" content="{SITE_URL}images/shr-daw-physical-connections.jpg">\n',
        '<meta property="og:image:type" content="image/jpeg">\n',
        f'<meta property="og:image:width" content="{social_width}">\n',
        f'<meta property="og:image:height" content="{social_height}">\n',
        '<meta property="og:image:alt" content="SHR-DAW Raspberry Pi mini DAW physical connection diagram">\n',
        '<meta name="twitter:card" content="summary_large_image">\n',
        '<meta name="twitter:title" content="SHR-DAW — Raspberry Pi mini DAW">\n',
        f'<meta name="twitter:description" content="{html.escape(intro, quote=True)}">\n',
        f'<meta name="twitter:image" content="{SITE_URL}images/shr-daw-physical-connections.jpg">\n',
        "<style>\n",
        CSS.strip(),
        "\n</style>\n",
        '<script>document.documentElement.className="js";</script>\n',
        "</head>\n<body>\n",
        '<a class="skip-link" href="#main-content">Skip to documentation</a>\n',
        '<header class="site-header">',
        '<a class="brand" href="#top"><span class="leds" aria-hidden="true">'
        '<span class="led"></span><span class="led yellow"></span>'
        '<span class="led red"></span></span><span>SHR-DAW</span></a>',
        '<div class="header-links"><a href="#start-here">Start here</a>'
        f'<a class="external" href="{REPOSITORY_URL}" target="_blank" '
        'rel="noopener noreferrer external">Source</a>'
        '<button class="nav-toggle" type="button" aria-expanded="false" '
        'aria-controls="site-nav">Documentation</button></div></header>\n',
        '<div class="site-shell">\n',
        '<aside class="site-nav" id="site-nav" aria-label="Documentation navigation">',
        '<h2>Documentation</h2>',
        '<div class="search-box"><label for="doc-search">Search this page</label>'
        '<input id="doc-search" type="search" autocomplete="off" '
        'placeholder="Try “loops” or “JACK”">'
        '<p class="search-help">Press / to search</p>'
        '<p class="sr-only" id="search-status" aria-live="polite">'
        "Search all headings and document text.</p>"
        '<ul id="search-results"></ul></div>',
        "<noscript><p>Search needs JavaScript; all navigation and documentation remain available below.</p></noscript>",
        nav_html(groups),
        "</aside>\n",
        '<main id="main-content">\n',
        '<section class="hero" id="top"><div class="hero-copy">'
        '<p class="eyebrow">Raspberry Pi music workstation</p>'
        "<h1>SHR-DAW</h1>"
        f'<p class="intro">{html.escape(intro)}</p>'
        '<p class="status-line"><span class="led" aria-hidden="true"></span>'
        f'<span class="version">Version {html.escape(version)}</span>'
        "<span>40×13 interface · FT2-style tracker · JACK recording</span></p>"
        '<div class="button-row"><a class="button" href="#start-here">Start here</a>'
        f'<a class="button secondary external" href="{REPOSITORY_URL}" '
        'target="_blank" rel="noopener noreferrer external">View source</a></div>'
        "</div>",
        image_html(first_image, eager=True).replace('class="', 'class="hero-image ', 1)
        if first_image.attrGet("class")
        else image_html(first_image, eager=True).replace("<img ", '<img class="hero-image" ', 1),
        "</section>\n",
        '<section class="overview-block" aria-labelledby="features-title">'
        '<p class="eyebrow">From the current README</p>'
        '<h2 id="features-title">Compact workstation, focused controls</h2>'
        f'<div class="feature-grid">{feature_cards}</div></section>\n',
        '<section class="overview-block" id="start-here" aria-labelledby="start-title">'
        '<p class="eyebrow">Musicians first</p><h2 id="start-title">Start here</h2>'
        f'<div class="start-grid">{start_cards}</div></section>\n',
        '<section class="overview-block" id="screenshots" aria-labelledby="screenshots-title">'
        '<p class="eyebrow">Real 40×13 interface</p>'
        '<h2 id="screenshots-title">Screens from the application</h2>'
        f'<div class="screenshot-grid">{screenshot_cards}</div></section>\n',
        content_html(groups),
        '<footer class="page-footer"><p>Generated from the public SHR-DAW repository sources. '
        f'<a href="#doc-license">MIT licence</a> · '
        '<a href="#doc-third-party">Third-party provenance</a> · '
        f'<a class="external" href="{REPOSITORY_URL}" target="_blank" '
        'rel="noopener noreferrer external">Repository source</a></p></footer>',
        "</main>\n</div>\n",
        "<script>\n",
        JS.strip(),
        "\n</script>\n</body>\n</html>\n",
    ]
    return "".join(parts)


def cargo_version() -> str:
    cargo = ROOT / "Cargo.toml"
    if not cargo.is_file():
        fail("missing source: Cargo.toml")
    with cargo.open("rb") as handle:
        data = tomllib.load(handle)
    version = data.get("package", {}).get("version")
    if not isinstance(version, str) or not re.fullmatch(r"\d+\.\d+\.\d+", version):
        fail("Cargo.toml package.version is missing or unsupported")
    return version


def validate_generated(page: str, anchors: set[str], referenced_images: set[Path]) -> None:
    if "<meta name=\"viewport\"" not in page:
        fail("generated page lacks viewport metadata")
    if any(
        "user/" in target
        for target in re.findall(r'(?:href|src)="([^"]*)"', page)
    ):
        fail("generated page links to private user storage")
    if re.search(r'(?:href|src)="file:', page, flags=re.IGNORECASE):
        fail("generated page contains a local file URL")
    if "javascript:" in page.casefold():
        fail("generated page contains a javascript: URL")
    for pattern in SECRET_PATTERNS:
        if pattern.search(page):
            fail("generated page contains credential-like content")
    ids = re.findall(r'\bid="([^"]+)"', page)
    duplicates = sorted({item for item in ids if ids.count(item) > 1})
    if duplicates:
        fail("duplicate generated anchor: " + ", ".join(duplicates))
    href_fragments = re.findall(r'href="#([^"]+)"', page)
    known = set(ids) | anchors
    broken = sorted({unquote(item) for item in href_fragments if unquote(item) not in known})
    if broken:
        fail("broken generated fragment links: " + ", ".join(broken))
    image_sources = re.findall(r'<img\b[^>]*\bsrc="([^"]+)"', page)
    for source in image_sources:
        parsed = urlsplit(html.unescape(source))
        if parsed.scheme or parsed.netloc:
            fail(f"generated body contains a remote image: {source}")
        target = (DOCS_DIR / unquote(parsed.path)).resolve()
        rel = relative_path(target)
        if rel not in referenced_images:
            fail(f"generated image was not validated from Markdown: {source}")
        if not target.is_file():
            fail(f"generated image does not exist: {source}")


def generate() -> str:
    check_dependencies()
    md = make_markdown()
    documents = discover_documents(md)
    anchors = assign_headings(documents)
    referenced_images = rewrite_tokens(documents, md)
    groups = documentation_groups(documents, md)
    page = build_page(documents, groups, cargo_version())
    validate_generated(page, anchors, referenced_images)
    return page


def write_atomic(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", newline="\n", dir=path.parent, delete=False
    ) as handle:
        temporary = Path(handle.name)
        handle.write(content)
    try:
        os.chmod(temporary, 0o644)
        os.replace(temporary, path)
    finally:
        if temporary.exists():
            temporary.unlink()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate or check docs/index.html from public Markdown sources."
    )
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument(
        "--write",
        action="store_true",
        help="atomically regenerate docs/index.html",
    )
    mode.add_argument(
        "--check",
        action="store_true",
        help="regenerate to temporary output and fail if docs/index.html drifts",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    page = generate()
    if args.write:
        write_atomic(OUTPUT, page)
        print(f"generated {OUTPUT.relative_to(ROOT)}")
        return 0
    if not OUTPUT.is_file():
        fail("docs/index.html is missing; run make docs-site")
    with tempfile.TemporaryDirectory(prefix="shr-docs-site-check-") as temp_dir:
        candidate = Path(temp_dir) / "index.html"
        candidate.write_text(page, encoding="utf-8", newline="\n")
        expected = OUTPUT.read_bytes()
        actual = candidate.read_bytes()
    if actual != expected:
        fail("docs/index.html is stale; run make docs-site")
    print(f"validated deterministic {OUTPUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
