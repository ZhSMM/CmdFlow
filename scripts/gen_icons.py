"""生成 CmdFlow 启动图标 - 占位用,后续可替换"""
from PIL import Image, ImageDraw, ImageFont
import os

OUT_DIR = r"C:/Users/19114/.minimax-agent-cn/projects/CmdFlow/src-tauri/icons"
os.makedirs(OUT_DIR, exist_ok=True)

# 源图: 1024x1024
SRC_SIZE = 1024
src = Image.new("RGBA", (SRC_SIZE, SRC_SIZE), (0, 0, 0, 0))
draw = ImageDraw.Draw(src)

# 圆角矩形背景 - 深蓝
margin = 80
draw.rounded_rectangle(
    [margin, margin, SRC_SIZE - margin, SRC_SIZE - margin],
    radius=180,
    fill=(15, 23, 42, 255),  # slate-900
)

# 终端符号 ">_"
try:
    font_large = ImageFont.truetype("consola.ttf", 480)
except OSError:
    font_large = ImageFont.load_default()

text = ">_"
bbox = draw.textbbox((0, 0), text, font=font_large)
text_w = bbox[2] - bbox[0]
text_h = bbox[3] - bbox[1]
text_x = (SRC_SIZE - text_w) / 2 - bbox[0]
text_y = (SRC_SIZE - text_h) / 2 - bbox[1] - 30

# 阴影
draw.text((text_x + 8, text_y + 8), text, font=font_large, fill=(0, 0, 0, 80))
# 主文字 - 翠绿
draw.text((text_x, text_y), text, font=font_large, fill=(74, 222, 128, 255))  # green-400

# 保存源图
src.save(os.path.join(OUT_DIR, "icon.png"))

# 生成多尺寸
sizes = {
    "32x32.png": 32,
    "128x128.png": 128,
    "128x128@2x.png": 256,
    "icon-512.png": 512,
}

for name, size in sizes.items():
    img = src.resize((size, size), Image.LANCZOS)
    img.save(os.path.join(OUT_DIR, name))

# 生成 .ico (Windows 资源文件用,内嵌多个尺寸)
ico_sizes = [(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
src.save(os.path.join(OUT_DIR, "icon.ico"), sizes=ico_sizes, append_images=[
    src.resize(s, Image.LANCZOS) for s in ico_sizes[1:]
])

# macOS icns - 用 sips 之类工具,这里跳过 (跨平台后置)

print("✅ 图标生成完成:")
for f in sorted(os.listdir(OUT_DIR)):
    path = os.path.join(OUT_DIR, f)
    print(f"  {f} ({os.path.getsize(path)} bytes)")
