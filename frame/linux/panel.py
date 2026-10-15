"""The frame's panel for mainline's Jadard JD9365DA driver, generated from the vendor's device tree
(the backup's vendor-0.dts, never in the repo): its 198 DSI commands in order with their delays,
its timings. Rockchip's 3.10 kernel lists them as `rockchip,cmd = <type reg value…>` (0x15: one
parameter, 0x05: none, 0x39: long) with `rockchip,cmd_delay` in ms, under `screen-on-cmds`.

    python3 frame/linux/panel.py ~/maison-frame/backup/vendor-0.dts > <the C to splice>
"""

import re
import sys

# DCS commands mainline names: the vendor's sleep out and display on
EXIT_SLEEP, DISPLAY_ON, SWITCH_PAGE = 0x11, 0x29, 0xE0
STANDARD = [(0xE1, 0x93), (0xE2, 0x65), (0xE3, 0xF8), (0x80, 0x03)]  # jadard_enable_standard_cmds, 4 lanes


def commands(dts: str) -> list[tuple[int, tuple[int, ...], int]]:
    """(type, bytes, delay ms) for each vendor command, in the order the panel wants them."""
    out = []
    for m in re.finditer(r"rockchip,on-cmds\d+ \{(.*?)\};", dts, re.S):
        body = m.group(1)
        cmd = [int(x, 16) for x in re.search(r"rockchip,cmd = <([^>]*)>", body).group(1).split()]
        delay = int(re.search(r"rockchip,cmd_delay = <([^>]*)>", body).group(1), 16)
        out.append((cmd[0], tuple(cmd[1:]), delay))
    if not out:
        raise SystemExit("no rockchip,on-cmds: not the vendor's device tree?")
    return out


def timing(dts: str) -> dict[str, int]:
    block = dts[dts.index("display-timings") :]
    get = lambda key: int(re.search(rf"\b{key} = <([^>]*)>", block).group(1), 16)
    return {k: get(k) for k in ("clock-frequency", "hactive", "vactive", "hsync-len", "hback-porch", "hfront-porch", "vsync-len", "vback-porch", "vfront-porch")}


def c_init(cmds) -> list[str]:
    lines, standard = [], list(STANDARD)
    for kind, data, delay in cmds:
        if kind == 0x05 and data == (EXIT_SLEEP,):
            lines.append("\tmipi_dsi_dcs_exit_sleep_mode_multi(&dsi_ctx);")
        elif kind == 0x05 and data == (DISPLAY_ON,):
            lines.append("\tmipi_dsi_dcs_set_display_on_multi(&dsi_ctx);")
        elif data[:1] == (SWITCH_PAGE,) and len(data) == 2:
            lines.append(f"\n\tjd9365da_switch_page(&dsi_ctx, 0x{data[1]:02x});")
        elif standard and data == standard[0]:
            # the driver's own password and lane setting, said once
            standard.pop(0)
            if not standard:
                lines.append("\tjadard_enable_standard_cmds(&dsi_ctx);")
        else:
            lines.append("\tmipi_dsi_dcs_write_seq_multi(&dsi_ctx, " + ", ".join(f"0x{b:02x}" for b in data) + ");")
        if delay:
            lines.append(f"\tmipi_dsi_msleep(&dsi_ctx, {delay});")
    if standard:
        raise SystemExit("the vendor's init lacks the JD9365DA's standard commands: another controller?")
    return lines


def main() -> int:
    dts = open(sys.argv[1]).read()
    t = timing(dts)
    clock = t["clock-frequency"] // 1000
    h, v = t["hactive"], t["vactive"]
    print("static int amaboo_gbzk_y1000_init_cmds(struct jadard *jadard)\n{")
    print("\tstruct mipi_dsi_multi_context dsi_ctx = { .dsi = jadard->dsi };\n")
    print("\n".join(c_init(commands(dts))).lstrip("\n"))
    print("\n\treturn dsi_ctx.accum_err;\n};\n")
    print("static const struct jadard_panel_desc amaboo_gbzk_y1000_desc = {")
    print("\t.mode_4ln = &(const struct drm_display_mode) {")
    print(f"\t\t.clock\t\t= {clock},\n")
    print(f"\t\t.hdisplay\t= {h},")
    print(f"\t\t.hsync_start\t= {h} + {t['hfront-porch']},")
    print(f"\t\t.hsync_end\t= {h} + {t['hfront-porch']} + {t['hsync-len']},")
    print(f"\t\t.htotal\t\t= {h} + {t['hfront-porch']} + {t['hsync-len']} + {t['hback-porch']},\n")
    print(f"\t\t.vdisplay\t= {v},")
    print(f"\t\t.vsync_start\t= {v} + {t['vfront-porch']},")
    print(f"\t\t.vsync_end\t= {v} + {t['vfront-porch']} + {t['vsync-len']},")
    print(f"\t\t.vtotal\t\t= {v} + {t['vfront-porch']} + {t['vsync-len']} + {t['vback-porch']},\n")
    # a 10.1-inch 16:10 panel
    print("\t\t.width_mm\t= 135,\n\t\t.height_mm\t= 216,")
    print("\t\t.type\t\t= DRM_MODE_TYPE_DRIVER | DRM_MODE_TYPE_PREFERRED,\n\t},")
    print("\t.lanes = 4,\n\t.format = MIPI_DSI_FMT_RGB888,\n\t.init = amaboo_gbzk_y1000_init_cmds,\n};")
    return 0


if __name__ == "__main__":
    sys.exit(main())
