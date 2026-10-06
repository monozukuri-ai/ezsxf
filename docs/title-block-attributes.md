# Title-block attribute names

SXF represents title-block information in two related places. The
`drawing_attribute_feature` stores the drawing-wide values and is returned in
`typed_features` with `kind == "drawing_attribute"`. Text drawn in the title
block is linked to those values by an `ATRS` attribute attachment. By contrast,
`drawing_sheet_feature` (`kind == "drawing_sheet"`) describes the paper size
and orientation; `model["sheet"]` is the resolved paper/container structure,
not the title-block metadata.

The following strings are the exact, machine-readable predefined attribute
names used in an `ATRS` attachment. They are case-sensitive literals and must
not be translated or replaced by the `S-xx` catalogue identifier. All eleven
attributes have the predefined SXF type `STR`.

| Catalogue ID | Exact `attribute_name` | SFC field / Python key | Meaning |
| --- | --- | --- | --- |
| `S-05` | `表題_事業名` | `P_Name` / `project_name` | Project name |
| `S-06` | `表題_工事名` | `C_Name` / `construction_name` | Construction name |
| `S-07` | `表題_契約区分` | `C_type` / `contract_type` | Contract type |
| `S-08` | `表題_図面番号` | `D_number` / `drawing_number` (before `$$`) | Drawing number |
| `S-09` | `表題_図面総数` | `D_number` / `drawing_number` (after `$$`) | Total drawing count |
| `S-10` | `表題_図面種別` | `D_type` / `drawing_type` | Drawing type |
| `S-11` | `表題_尺度` | `D_Scale` / `drawing_scale` | Scale |
| `S-12` | `表題_図面名` | `D_title` / `drawing_name` | Drawing name |
| `S-13` | `表題_年月日` | `D_Year`, `D_Month`, `D_Day` / `drawing_year`, `drawing_month`, `drawing_day` | Drawing date |
| `S-14` | `表題_会社名` | `C_Contractor` / `contractor_name` | Contractor name |
| `S-15` | `表題_事務所名` | `C_Owner` / `owner_name` | Owner/commissioning organization name |

An explicit-type attachment name has this form:

```text
$$ATRS$$<figure-id>$$<attribute-name>$$STR
```

For example, `$$ATRS$$9$$表題_事業名$$STR` is exposed at
`model["attribute_attachments"][...]["attribute"]` as:

```python
{
    "mechanism": "ATRS",
    "figure_id": "9",
    "attribute_name": "表題_事業名",
    "attribute_type": "STR",
    "unit": None,
}
```

Because `ATRS` applies to a text feature, that feature's displayed text is the
attribute value; there is no separate `attribute_value` key. The type and unit
may be omitted from the encoded name, in which case their parsed values are
`None`.

For a title-block value drawn on multiple lines, use the unsuffixed predefined
name for a single line. For multiple lines, the SXF specification's published
form appends an ASCII space and a 1-based ASCII line number, for example
`表題_工事名 1` and `表題_工事名 2`. `ezsxf` preserves this suffix verbatim and
does not fold the line-specific names back to the base name.

There are two compound-field rules to keep separate from the `ATRS` names:

- `D_number` / `drawing_number` stores `<drawing-number>$$<total-count>` when a
  total count is present, while title-block text uses separate
  `表題_図面番号` and `表題_図面総数` attachments.
- `表題_年月日` is one text attribute, while `drawing_attribute_feature`
  exposes its date as the three integer keys `drawing_year`, `drawing_month`,
  and `drawing_day`.
