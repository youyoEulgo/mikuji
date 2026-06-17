#!/bin/bash
# 测试 Kitty 图形协议是否工作

# 创建一个简单的红色方块 PNG
cat > /tmp/test_kitty.png << 'EOF' | base64 -d
iVBORw0KGgoAAAANSUhEUgAAAAoAAAAKCAYAAACNMs+9AAAAFUlEQVR42mP8z8BQz0AEYBxVSF+FABJADveWkH6oAAAAAElFTkSuQmCC
EOF

# 使用 Kitty 协议显示
PNG_DATA=$(base64 < /tmp/test_kitty.png | tr -d '\n')
echo -ne "\x1b_Ga=T,f=100;${PNG_DATA}\x1b\\"
echo ""
echo "如果上面显示了一个红色方块，说明 Kitty 协议工作正常"
echo "如果什么都没有，说明终端不支持 Kitty 图形协议"
