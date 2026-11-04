using System.Buffers.Binary;
using System.Diagnostics;
using System.Net.Sockets;
using System.Text;

namespace Bai7_8;

// Client MQTT 5.0 tối giản trên socket TCP (cài đặt các lệnh mức thư viện của Mã giả 7.9):
// CONNECT (kèm di chúc) / CONNACK, PUBLISH QoS 0 và 1 (PUBACK), SUBSCRIBE / SUBACK, PINGREQ / PINGRESP, DISCONNECT.
// Không hỗ trợ QoS 2, TLS, thuộc tính MQTT 5.0 (luôn gửi độ dài thuộc tính 0).
// Định khung: 1 byte loại gói + cờ, "độ dài còn lại" varint 1–4 byte, rồi đúng chừng ấy byte (RECV_EXACT).
sealed class MqttClient
{
    const int Connect_ = 1, Connack = 2, Publish = 3, Puback = 4, Subscribe = 8, Suback = 9,
              Pingreq = 12, Pingresp = 13, Disconnect_ = 14;
    Socket sock = null!;
    int nextId = 1, keepAlive;
    readonly Stopwatch lastSend = new();
    readonly List<(string Filter, Action<string, string, int, bool> Fn)> handlers = [];

    static byte[] Str(string s)                                         // chuỗi UTF-8: 2 byte độ dài + dữ liệu
    {
        byte[] b = Encoding.UTF8.GetBytes(s);
        return [(byte)(b.Length >> 8), (byte)b.Length, .. b];
    }

    static byte[] Varint(int n)                                         // 7 bit mỗi byte, bit cao = còn byte tiếp
    {
        var o = new List<byte>();
        do { byte d = (byte)(n & 0x7F); n >>= 7; o.Add((byte)(d | (n > 0 ? 0x80 : 0))); } while (n > 0);
        return [.. o];
    }

    public static bool TopicMatches(string flt, string topic)          // '+' một cấp, '#' mọi cấp còn lại
    {
        string[] f = flt.Split('/'), t = topic.Split('/');
        for (int i = 0; i < f.Length; i++)
        {
            if (f[i] == "#") return true;
            if (i >= t.Length || (f[i] != "+" && f[i] != t[i])) return false;
        }
        return f.Length == t.Length;
    }

    void Send(int type, int flags, byte[] body)
    {
        sock.Send([(byte)((type << 4) | flags), .. Varint(body.Length), .. body]);   // Send chặn gửi hết
        lastSend.Restart();                                             // keep alive tính theo gói client GỬI
    }

    byte[] RecvExact(int n)                                             // Mã giả 3.4 – RECV_EXACT
    {
        var buf = new byte[n]; int have = 0;
        while (have < n)
        {
            int k = sock.Receive(buf, have, n - have, SocketFlags.None);
            if (k == 0) throw new IOException("Broker đóng kết nối");
            have += k;
        }
        return buf;
    }

    // Đọc một gói; chỉ chờ byte đầu với firstTimeoutMs (hết giờ ở đây chưa mất byte nào của gói)
    (int Type, int Flags, byte[] Body) ReadPacket(int firstTimeoutMs = 5000)
    {
        sock.ReceiveTimeout = firstTimeoutMs;
        byte b0 = RecvExact(1)[0];
        sock.ReceiveTimeout = 5000;
        int n = 0, shift = 0; byte d;
        do { d = RecvExact(1)[0]; n |= (d & 0x7F) << shift; shift += 7; } while ((d & 0x80) != 0);
        return (b0 >> 4, b0 & 0x0F, RecvExact(n));
    }

    void Dispatch(int flags, byte[] body)                               // gói PUBLISH đến
    {
        int qos = (flags >> 1) & 3; bool retain = (flags & 1) != 0;
        int tlen = BinaryPrimitives.ReadUInt16BigEndian(body), i = 2 + tlen;
        string topic = Encoding.UTF8.GetString(body, 2, tlen);
        if (qos > 0) { Send(Puback, 0, body[i..(i + 2)]); i += 2; }    // QoS 1: xác nhận cho broker
        int plen = 0, shift = 0; byte d;
        do { d = body[i++]; plen |= (d & 0x7F) << shift; shift += 7; } while ((d & 0x80) != 0);
        string payload = Encoding.UTF8.GetString(body, i + plen, body.Length - i - plen);   // bỏ qua thuộc tính
        foreach (var (flt, fn) in handlers)
            if (TopicMatches(flt, topic)) fn(topic, payload, qos, retain);
    }

    byte[] Wait(int type)                                               // chờ gói; PUBLISH đến giữa chừng vẫn xử lý
    {
        while (true)
        {
            var (t, flags, body) = ReadPacket();
            if (t == type) return body;
            if (t == Publish) Dispatch(flags, body);
        }
    }

    // MQTT_CONNECT; will = (chủ_đề, nội_dung, qos, retain)
    public void Connect(string host, int port, string clientId, int keepAliveS = 60,
                        (string Topic, string Msg, int Qos, bool Retain)? will = null)
    {
        var tcp = new TcpClient();
        if (!tcp.ConnectAsync(host, port).Wait(5000)) throw new IOException("Hết thời gian kết nối");
        sock = tcp.Client; keepAlive = keepAliveS;
        int flags = 0x02;                                               // Clean Start
        byte[] payload = Str(clientId);
        if (will is { } w)
        {
            flags |= 0x04 | (w.Qos << 3) | (w.Retain ? 0x20 : 0);
            payload = [.. payload, 0, .. Str(w.Topic), .. Str(w.Msg)];  // thuộc tính di chúc (0), chủ đề, nội dung
        }
        Send(Connect_, 0, [.. Str("MQTT"), 5, (byte)flags, (byte)(keepAliveS >> 8), (byte)keepAliveS, 0, .. payload]);
        byte[] ack = Wait(Connack);
        if (ack[1] != 0) throw new IOException($"CONNACK mã lý do 0x{ack[1]:X2}");   // 0x00: thành công
    }

    public void PublishMsg(string topic, string payload, int qos = 0, bool retain = false)
    {
        byte[] body = Str(topic);
        if (qos > 0) { int pid = nextId++; body = [.. body, (byte)(pid >> 8), (byte)pid]; }
        Send(Publish, (qos << 1) | (retain ? 1 : 0), [.. body, 0, .. Encoding.UTF8.GetBytes(payload)]);
        if (qos == 1) Wait(Puback);                                     // QoS 1: chờ broker xác nhận
    }

    public int SubscribeTo(string flt, int qos, Action<string, string, int, bool> handler)
    {
        handlers.Add((flt, handler));                                   // đăng ký trước: tin giữ lại có thể đến ngay
        int pid = nextId++;
        Send(Subscribe, 0x2, [(byte)(pid >> 8), (byte)pid, 0, .. Str(flt), (byte)qos]);
        byte[] ack = Wait(Suback);
        int code = ack[3 + ack[2]];                                     // sau mã gói và thuộc tính
        if (code >= 0x80) throw new IOException($"SUBACK từ chối {flt}: 0x{code:X2}");
        return code;
    }

    // RUN_EVENT_LOOP: nhận gói, gọi hàm xử lý; chưa gửi gì trong keep_alive/2 giây thì gửi PINGREQ
    public void RunEventLoop(Action? onPing = null)
    {
        while (true)
        {
            int wait = keepAlive * 500 - (int)lastSend.ElapsedMilliseconds;
            if (wait <= 0) { Send(Pingreq, 0, []); continue; }
            (int Type, int Flags, byte[] Body) p;
            try { p = ReadPacket(wait); }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.TimedOut) { continue; }
            if (p.Type == Publish) Dispatch(p.Flags, p.Body);
            else if (p.Type == Pingresp) onPing?.Invoke();
        }
    }

    public void Disconnect()
    {
        Send(Disconnect_, 0, []);                                       // mã lý do 0: broker bỏ di chúc
        sock.Close();
    }

    public void CloseAbruptly() => sock.Close();                        // đóng socket, không gửi DISCONNECT
}
