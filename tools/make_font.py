"""Noto Sans JP（可変フォント）から実機用のサブセットを作る。

  pip install fonttools
  python3 tools/make_font.py "NotoSansJP[wght].ttf" firmware/host/assets/NotoSansJP-Regular-jis.ttf

wght=400 に固定し、ASCII / Latin-1 / 一般的な記号 / JIS X 0208（かな・JIS 第1・第2水準漢字）
だけを残す。元フォントは https://github.com/google/fonts/tree/main/ofl/notosansjp （OFL）。
"""
import sys

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer


def jis_x_0208() -> set[int]:
    cps = set()
    for hi in range(0xA1, 0xFF):
        for lo in range(0xA1, 0xFF):
            try:
                ch = bytes([hi, lo]).decode("euc_jp")
            except UnicodeDecodeError:
                continue
            cps.update(ord(c) for c in ch)
    return cps


def main(src: str, dst: str) -> None:
    font = TTFont(src)
    font = instancer.instantiateVariableFont(font, {"wght": 400})
    cps = set(range(0x20, 0x7F)) | set(range(0xA0, 0x100)) | jis_x_0208()
    cps |= set(range(0x2000, 0x2070)) | set(range(0x2190, 0x2200)) | set(range(0x25A0, 0x2700))
    cps |= set(range(0x3000, 0x3040)) | set(range(0xFF01, 0xFF9F)) | {0x00B0, 0x2103}
    opts = subset.Options()
    opts.layout_features = ["kern", "palt"]
    opts.hinting = False
    opts.desubroutinize = True
    opts.name_IDs = ["*"]
    sub = subset.Subsetter(opts)
    sub.populate(unicodes=sorted(cps))
    sub.subset(font)
    font.save(dst)
    print(f"{dst}: {len(cps)} code points")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
