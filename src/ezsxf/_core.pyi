from os import PathLike
from typing import Any

def hello_from_bin() -> str: ...
def parse_p21(input: str | bytes, strict: bool = True) -> dict[str, Any]: ...
def parse_sfc(input: str | bytes, strict: bool = True) -> dict[str, Any]: ...
def serialize_sfc(
    parsed: dict[str, Any], *, allow_external_references: bool = False
) -> bytes: ...
def write_sfc(
    parsed: dict[str, Any],
    path: str | PathLike[str],
    *,
    allow_external_references: bool = False,
) -> None: ...
def write_sfc_bundle(
    source: str | PathLike[str],
    destination: str | PathLike[str],
    *,
    file_name: str | None = None,
    extra_files: list[str] | None = None,
) -> dict[str, Any]: ...
def new_sfc(
    file_name: str = "drawing.sfc",
    *,
    name: str = "drawing",
    width_mm: int = 297,
    height_mm: int = 210,
    timestamp: str | None = None,
) -> SfcDocument: ...
def edit_sfc(parsed: dict[str, Any]) -> SfcDocument: ...
def edit_sfc_bundle(source: str | PathLike[str]) -> SfcDocument: ...
def validate_saf(data: bytes) -> None: ...

class SfcDocument:
    def save_bundle(self, destination: str | PathLike[str], *, file_name: str | None = None) -> dict[str, Any]: ...
    def saf_bytes(self) -> bytes | None: ...
    def set_attribute(self, entity_id: int, name: str, value: str, *, attribute_type: str = "STR", unit: str | None = None, group: list[str] | None = None, set_name: str = "ezsxf attributes", set_version: str = "1.0", designed_by: str = "ezsxf", figure_name: str = "figure") -> str: ...
    def get_attributes(self, entity_id: int) -> list[dict[str, Any]]: ...
    def remove_attribute(self, entity_id: int, name: str, *, group: list[str] | None = None, set_name: str = "ezsxf attributes", set_version: str = "1.0", designed_by: str = "ezsxf") -> None: ...
    def remove_attachment(self, entity_id: int) -> None: ...
    def set_single_attribute(self, entity_id: int, figure_name: str, name: str, value: str, *, attribute_type: str = "STR", unit: str | None = None) -> str: ...
    def set_text_attribute(self, entity_id: int, name: str, *, attribute_type: str | None = None, unit: str | None = None) -> str: ...
    def add_dependency(self, source: str | PathLike[str], *, file_name: str | None = None) -> str: ...
    def add_image(self, image: str | PathLike[str], anchor: tuple[float, float], width_mm: float, height_mm: float, *, angle: float = 0.0, layer: int = 1, file_name: str | None = None) -> int: ...
    def update_image(self, entity_id: int, anchor: tuple[float, float], width_mm: float, height_mm: float, *, angle: float = 0.0, image: str | PathLike[str] | None = None, file_name: str | None = None) -> None: ...
    def to_dict(self) -> dict[str, Any]: ...
    def to_bytes(self, *, allow_external_references: bool = False) -> bytes: ...
    def save(
        self, path: str | PathLike[str], *, allow_external_references: bool = False
    ) -> None: ...
    def add_layer(self, name: str, *, visible: bool = True) -> int: ...
    def rename_layer(self, code: int, name: str) -> None: ...
    def add_font(self, name: str) -> int:
        """Return the existing exact-name font code, or append a new font."""
        ...
    def add_line(
        self,
        start: tuple[float, float],
        end: tuple[float, float],
        *,
        layer: int = 1,
        color: int = 1,
        line_type: int = 1,
        line_width: int = 1,
    ) -> int: ...
    def add_circle(
        self,
        center: tuple[float, float],
        radius: float,
        *,
        layer: int = 1,
        color: int = 1,
        line_type: int = 1,
        line_width: int = 1,
    ) -> int: ...
    def add_arc(
        self,
        center: tuple[float, float],
        radius: float,
        start_angle: float,
        end_angle: float,
        *,
        direction: int = 0,
        layer: int = 1,
        color: int = 1,
        line_type: int = 1,
        line_width: int = 1,
    ) -> int: ...
    def add_polyline(
        self,
        points: list[tuple[float, float]],
        *,
        layer: int = 1,
        color: int = 1,
        line_type: int = 1,
        line_width: int = 1,
    ) -> int: ...
    def add_text(
        self,
        text: str,
        anchor: tuple[float, float],
        *,
        height: float = 3.5,
        width: float = 3.5,
        spacing: float = 0.0,
        angle: float = 0.0,
        slant: float = 0.0,
        base_point: int = 1,
        direction: int = 1,
        layer: int = 1,
        color: int = 1,
        font: int = 1,
    ) -> int: ...
    def update_element(self, entity_id: int, **changes: Any) -> None: ...
    def remove_element(self, entity_id: int) -> None: ...
