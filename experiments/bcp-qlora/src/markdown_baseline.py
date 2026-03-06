"""
Markdown baseline builders for comparison with BCP-rendered output.

=== Why Do We Need This? ===
To measure whether BCP improves comprehension, we need a baseline: "how well
does the model understand the SAME code when presented as traditional markdown?"
This module builds that markdown baseline.

=== Two Baselines ===
We build two markdown variants to bracket the range of real-world formatting:

1. Naive Markdown: Triple-backtick code fences with language tags.
   This is how most tools dump code into prompts.

2. Realistic Agent Markdown: XML-style tags + JSON envelopes, mimicking
   how Claude Code actually formats context (more verbose = more tokens).

The BCP Minimal format should use fewer tokens than BOTH baselines while
preserving the same semantic content.
"""


def build_naive_markdown(files: list[dict]) -> str:
    """
    Build naive markdown — triple backticks with language tags.

    This is the most common way tools present code to models.
    Each file gets a fenced code block with a comment showing the path.

    Args:
        files: List of file dicts with keys: language, path, content.

    Returns:
        A string of markdown-formatted code blocks.
    """
    parts = []
    for f in files:
        lang = f["language"]
        path = f["path"]
        content = f["content"]
        parts.append(f"```{lang}\n// {path}\n{content}\n```\n")
    return "\n".join(parts)


def build_realistic_markdown(files: list[dict]) -> str:
    """
    Build realistic agent markdown — mimics how Claude Code formats context.

    This includes XML-style tags, full paths repeated in multiple positions,
    and language identifiers in both the tag and the code fence. This is
    intentionally verbose because that's what models actually receive today.

    Args:
        files: List of file dicts with keys: language, path, content.

    Returns:
        A string of XML-tagged, fenced code blocks.
    """
    parts = ["<context>\n"]
    for f in files:
        lang = f["language"]
        path = f["path"]
        content = f["content"]
        parts.append(
            f'<source path="{path}" language="{lang}">\n'
            f"```{lang}\n{content}\n```\n"
            f"</source>\n\n"
        )
    parts.append("</context>\n")
    return "".join(parts)
