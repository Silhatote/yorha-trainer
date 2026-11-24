"""Build NieRAutomata_cheats.CT from the .cea script sources in this folder."""
from pathlib import Path
from xml.sax.saxutils import escape

SRC = Path(__file__).resolve().parent
OUT = SRC / "NieRAutomata_cheats.CT"


def script_entry(id_, desc, src, children="", hide_children=False):
    body = escape((SRC / src).read_text(encoding="utf-8"))
    opts = '\n      <Options moHideChildren="1"/>' if hide_children else ""
    kids = f"\n      <CheatEntries>{children}\n      </CheatEntries>" if children else ""
    return f"""
    <CheatEntry>
      <ID>{id_}</ID>
      <Description>"{escape(desc)}"</Description>{opts}
      <VariableType>Auto Assembler Script</VariableType>
      <AssemblerScript>{body}</AssemblerScript>{kids}
    </CheatEntry>"""


def value_entry(id_, desc, vtype, address):
    return f"""
        <CheatEntry>
          <ID>{id_}</ID>
          <Description>"{escape(desc)}"</Description>
          <VariableType>{vtype}</VariableType>
          <Address>{address}</Address>
        </CheatEntry>"""


entries = (
    script_entry(1, "Auto-Collect Items without the chip", "auto_collect.cea")
    + script_entry(
        2, "Auto-Collect range", "auto_collect_range.cea", hide_children=True,
        children=value_entry(3, "Range (game default 4.0)", "Float", "autoLootRange"),
    )
    + script_entry(
        4, "Respawn killed enemies (F6 = respawn last again)", "respawn.cea", hide_children=True,
        children=value_entry(5, "Auto respawn (1 = on, 0 = only F6)", "4 Bytes", "respawnAuto")
        + value_entry(6, "Respawn delay (seconds)", "Float", "respawnDelay")
        + value_entry(9, "Respawn where it died (1) / at original spawn (0)", "4 Bytes", "respawnAtDeath"),
    )
    + script_entry(
        7, "Item stack limit (replaces 99)", "item_cap.cea", hide_children=True,
        children=value_entry(8, "Stack limit", "4 Bytes", "itemCap"),
    )
)

OUT.write_text(
    f'<?xml version="1.0" encoding="utf-8"?>\n<CheatTable CheatEngineTableVersion="46">\n'
    f"  <CheatEntries>{entries}\n  </CheatEntries>\n</CheatTable>\n",
    encoding="utf-8",
)
print(f"wrote {OUT}")
