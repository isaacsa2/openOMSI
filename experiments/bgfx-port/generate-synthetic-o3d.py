#!/usr/bin/env python3
"""Generate a tiny **synthetic**, freely redistributable O3D fixture.

This is NOT original game data. Useful for a shader + native BGFX draw smoke
on a Windows CI runner that does not have OMSI 2 installed.
"""
import argparse
from pathlib import Path
from struct import pack

def make_triangle() -> bytes:
    out = bytearray([0x84, 0x19, 2])  # OMSI .o3d magic and version 2
    out += pack("<BH", 0x17, 3)   # 3 32-byte vertices
    # O3D file frame: x right, y up, z forward. The main engine converts
    # y<->z. A two-sided flat quad allows camera/winding validation.
    data = [
        (-2.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0),
        (2.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0),
        (0.0, 0.0, 4.0, 0.0, 1.0, 0.0, 0.5, 1.0),
    ]
    for row in data:
        out += pack("<8f", *row)
    out += pack("<BH", 0x49, 2)  # 2 triangle records (front and back)
    out += pack("<4H", 0, 1, 2, 0)
    out += pack("<4H", 0, 2, 1, 0)
    out += pack("<BH", 0x26, 1)  # 1 material
    out += pack("<4f", 1., 1., 1., 1.)  # diffuse RGBA
    out += pack("<3f", 0., 0., 0.)     # specular
    out += pack("<3f", 0., 0., 0.)     # emissive
    out += pack("<fB", 16., 0)        # specular exponent and empty tex name
    return bytes(out)

def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("output", type=Path)
    args = p.parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(make_triangle())
    print(f"Generated synthetic BGFX test O3D: {args.output}")

if __name__ == "__main__":
    main()
