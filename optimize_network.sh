#!/bin/bash
# optimize_network.sh - Otimizações de rede para baixa latência

echo "Aplicando otimizações de rede..."

# 1. Otimizações TCP
sudo sysctl -w net.core.rmem_max=134217728
sudo sysctl -w net.core.wmem_max=134217728
sudo sysctl -w net.ipv4.tcp_rmem="4096 87380 67108864"
sudo sysctl -w net.ipv4.tcp_wmem="4096 65536 67108864"
sudo sysctl -w net.ipv4.tcp_fastopen=3
sudo sysctl -w net.ipv4.tcp_syn_retries=2
sudo sysctl -w net.ipv4.tcp_synack_retries=2

# 2. BBR Congestion Control
sudo sysctl -w net.ipv4.tcp_congestion_control=bbr
sudo sysctl -w net.core.default_qdisc=fq

# 3. Desabilitar IPv6
sudo sysctl -w net.ipv6.conf.all.disable_ipv6=1
sudo sysctl -w net.ipv6.conf.default.disable_ipv6=1

# 4. Otimizações adicionais para baixa latência
sudo sysctl -w net.ipv4.tcp_low_latency=1
sudo sysctl -w net.ipv4.tcp_timestamps=0
sudo sysctl -w net.ipv4.tcp_sack=0
sudo sysctl -w net.core.netdev_max_backlog=5000

echo "Otimizações aplicadas!"
echo ""
echo "Para tornar permanente, adicione ao /etc/sysctl.conf:"
echo ""
echo "# Otimizações de rede para baixa latência"
echo "net.core.rmem_max = 134217728"
echo "net.core.wmem_max = 134217728"
echo "net.ipv4.tcp_rmem = 4096 87380 67108864"
echo "net.ipv4.tcp_wmem = 4096 65536 67108864"
echo "net.ipv4.tcp_fastopen = 3"
echo "net.ipv4.tcp_syn_retries = 2"
echo "net.ipv4.tcp_synack_retries = 2"
echo "net.ipv4.tcp_congestion_control = bbr"
echo "net.core.default_qdisc = fq"
echo "net.ipv6.conf.all.disable_ipv6 = 1"
echo "net.ipv6.conf.default.disable_ipv6 = 1"
echo "net.ipv4.tcp_low_latency = 1"
echo "net.ipv4.tcp_timestamps = 0"
echo "net.ipv4.tcp_sack = 0"
echo "net.core.netdev_max_backlog = 5000"

