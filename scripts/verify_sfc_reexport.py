"""Compare bounded SFC structure and referenced SAF/image data after CAD export.

Parsing and bundle validation use the Rust API. This read-only comparison is
not evidence that a CAD actually ran. Referenced image bytes must be identical;
recompression needs a separate decoded-pixel and visual review.
"""

import argparse
import hashlib
import json
import math
import xml.etree.ElementTree as ET
from collections import Counter
from pathlib import Path

import ezsxf

PRIMITIVES = {"line", "circle", "arc", "polyline", "text"}
CODE_FIELDS = {
    "layer_code": "layers",
    "color_code": "colors",
    "line_type_code": "line_types",
    "line_width_code": "line_widths",
    "font_code": "text_fonts",
}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def dependency(root: Path, name: str) -> Path:
    if not name or name in {".", ".."} or any(c in name for c in "/\\:"):
        raise ValueError("Only same-directory dependency basenames are supported")
    matches = [p for p in root.iterdir() if p.name.casefold() == name.casefold()]
    if len(matches) != 1 or not matches[0].is_file() or matches[0].is_symlink():
        raise ValueError(f"Missing, ambiguous or symbolic dependency: {name}")
    return matches[0]


def snapshot(path: Path) -> dict:
    parsed = ezsxf.parse_sfc(path.read_bytes(), strict=True)
    if parsed["warnings"]:
        raise ValueError("Strict SFC parsing reported warnings")
    # Also verify SAF figure bindings and raster rectangles with the Rust core.
    ezsxf.edit_sfc_bundle(path)
    model = parsed["model"]
    features = {f["id"]: f for f in parsed["typed_features"]}
    if len(features) != len(parsed["entities"]):
        raise ValueError("Untyped records require independent review")
    if model["composite_curve_definitions"] or model["hatch_references"]:
        raise ValueError("Composite curves/hatches are outside this comparison scope")
    if not model["sheet"]:
        raise ValueError("A drawing sheet is required")
    definitions = {d["entity_id"]: d for d in model["sfig_definitions"]}
    definitions.update({a["definition_id"]: a for a in model["attribute_attachments"]})
    names = {d["name"]: identifier for identifier, d in definitions.items()}
    tables = {
        name: {row["code"]: features[row["entity_id"]] for row in entries}
        for name, entries in model["code_tables"].items()
    }
    dependencies = {}
    saf_documents = {}
    visited = set()

    def referenced_file(name):
        file = dependency(path.parent, name)
        dependencies[file.name] = digest(file)
        return {"bytes_sha256": dependencies[file.name], "bytes": file.stat().st_size}

    def image(name):
        if Path(name).suffix.casefold() not in {".tif", ".tiff", ".jpg", ".jpeg"}:
            raise ValueError("Only TIFF/JPEG raster comparisons are supported")
        return referenced_file(name)

    def xml_node(node, sets):
        tag = node.tag.rsplit("}", 1)[-1]
        attributes = dict(node.attrib)
        if tag in {"Figure", "AttrSetRef"}:
            attributes.pop("id", None)
        result = {"tag": tag, "attributes": attributes}
        text = node.text or ""
        if tag == "Attr":
            name = attributes.get("name")
            result["value"] = (
                image(text)
                if name == "画像"
                else referenced_file(text)
                if name == "ファイル名"
                else text
            )
        elif text.strip():
            result["text"] = text
        if tag == "AttrSetRef":
            identifier = node.attrib["id"]
            if identifier not in sets:
                raise ValueError("Missing SAF attribute-set reference")
            result["set"] = sets[identifier]
        result["children"] = [xml_node(child, sets) for child in node]
        return result

    def saf_figure(attachment):
        file = dependency(path.parent, attachment["resolved_attribute_file_name"])
        if file.name not in saf_documents:
            data = file.read_bytes()
            ezsxf.validate_saf(data)
            root = ET.fromstring(data)
            sets = {
                (node.text or "").strip(): dict(node.attrib)
                for node in root
                if node.tag.rsplit("}", 1)[-1] == "AttributeSet"
            }
            figures = {
                node.attrib["id"]: xml_node(node, sets)
                for node in root
                if node.tag.rsplit("}", 1)[-1] == "Figure"
            }
            saf_documents[file.name] = figures
            dependencies[file.name] = digest(file)
        return saf_documents[file.name][attachment["attribute"]["figure_id"]]

    def fields(value):
        if isinstance(value, list):
            return [fields(item) for item in value]
        if not isinstance(value, dict):
            return value
        result = {}
        for key, item in value.items():
            if key in {"id", "keyword"}:
                continue
            if key in CODE_FIELDS:
                if item in (None, 0):
                    result[key] = item
                else:
                    table = tables[CODE_FIELDS[key]]
                    result[key] = fields(table[item])
            else:
                result[key] = fields(item)
        return result

    def node(identifier, stack=()):
        if identifier in stack:
            raise ValueError("Recursive composite figure")
        visited.add(identifier)
        feature = features[identifier]
        if feature["kind"] != "sfig_locate":
            if feature["kind"] not in PRIMITIVES:
                raise ValueError(f"Unsupported comparison feature: {feature['kind']}")
            return fields(feature)
        definition_id = names[feature["name"]]
        visited.add(definition_id)
        definition = definitions[definition_id]
        result = fields({k: v for k, v in feature.items() if k != "name"})
        result["kind_flag"] = definition["kind_flag"]
        result["children"] = [
            node(child, stack + (identifier,)) for child in definition["component_ids"]
        ]
        if "attribute" not in definition:
            result["name"] = definition["name"]
        else:
            attribute = dict(definition["attribute"])
            attribute.pop("figure_id", None)
            if attribute["mechanism"] == "ATRF":
                attribute.pop("attribute_file_name", None)
                attribute["figure"] = saf_figure(definition)
            elif attribute["mechanism"] == "ATRU" and attribute.get(
                "attribute_name"
            ) in {"画像", "ファイル名"}:
                file_reader = (
                    image if attribute["attribute_name"] == "画像" else referenced_file
                )
                attribute["attribute_value"] = file_reader(attribute["attribute_value"])
            result["attribute"] = attribute
        return result

    sheet = model["sheet"]
    sheet_feature = fields(features[sheet["entity_id"]])
    sheet_feature.pop("name", None)  # Output filename/sheet title may change.
    ordinary_groups = [d for d in definitions.values() if "attribute" not in d]
    canonical = {
        "sheet": sheet_feature,
        "children": [node(identifier) for identifier in sheet["component_ids"]],
        "groups": sorted(
            [
                {
                    "name": d["name"],
                    "kind_flag": d["kind_flag"],
                    "references": sum(
                        f["kind"] == "sfig_locate" and f["name"] == d["name"]
                        for f in features.values()
                    ),
                }
                for d in ordinary_groups
            ],
            key=lambda d: d["name"],
        ),
    }
    ignored = {sheet["entity_id"]} | {
        row["entity_id"] for entries in model["code_tables"].values() for row in entries
    }
    if set(features) - visited - ignored:
        raise ValueError("Unplaced definitions or geometry require independent review")
    return {
        "canonical": canonical,
        "drawing_sha256": digest(path),
        "dependencies_sha256": dependencies,
        "feature_counts": dict(Counter(f["kind"] for f in features.values())),
    }


def differences(before, after, path="", output=None):
    if output is None:
        output = []
    if isinstance(before, dict) and isinstance(after, dict):
        for key in sorted(before.keys() | after.keys()):
            if key not in before or key not in after:
                output.append(
                    {
                        "path": path + "/" + key,
                        "before": before.get(key),
                        "after": after.get(key),
                    }
                )
            else:
                differences(before[key], after[key], path + "/" + key, output)
    elif isinstance(before, list) and isinstance(after, list):
        if len(before) != len(after):
            output.append(
                {"path": path + "/length", "before": len(before), "after": len(after)}
            )
        for index, (left, right) in enumerate(zip(before, after)):
            differences(left, right, f"{path}/{index}", output)
    elif (
        isinstance(before, float)
        and isinstance(after, (int, float))
        or isinstance(after, float)
        and isinstance(before, (int, float))
    ):
        if not math.isclose(before, after, rel_tol=1e-9, abs_tol=1e-9):
            output.append({"path": path, "before": before, "after": after})
    elif before != after:
        output.append({"path": path, "before": before, "after": after})
    return output


def verify(source: Path, exported: Path) -> dict:
    report = {
        "passed": False,
        "evidence": "data-comparison-only",
        "image_check": "byte-exact",
    }
    try:
        before, after = snapshot(source), snapshot(exported)
        report.update(source=before, exported=after)
        delta = differences(before["canonical"], after["canonical"])
        report["differences"] = delta

        def content(value):
            if isinstance(value, list):
                return [content(item) for item in value]
            if not isinstance(value, dict):
                return value
            return {
                key: content(item)
                for key, item in value.items()
                if not (
                    key == "name"
                    and value.get("kind") == "sfig_locate"
                    and "attribute" not in value
                )
            }

        left, right = before["canonical"], after["canonical"]
        report["checks"] = {
            "sheet_parameters": not differences(left["sheet"], right["sheet"]),
            "geometry_hierarchy_styles_attributes_images": not differences(
                content(left["children"]), content(right["children"])
            ),
            "group_names_and_reuse": not differences(left["groups"], right["groups"]),
            "group_definition_reuse": sorted(
                (g["kind_flag"], g["references"]) for g in left["groups"]
            )
            == sorted((g["kind_flag"], g["references"]) for g in right["groups"]),
        }
        report["passed"] = not delta
    except (OSError, ValueError, KeyError, ET.ParseError) as error:
        report["error"] = str(error)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("exported", type=Path)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    if args.report.exists() or args.report.is_symlink():
        parser.error(
            "Report must be a new file; existing drawings/dependencies/reports are protected"
        )
    report = verify(args.source, args.exported)
    with args.report.open("x", encoding="utf-8") as output:
        output.write(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"passed": report["passed"], "report": str(args.report)}))
    if not report["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
