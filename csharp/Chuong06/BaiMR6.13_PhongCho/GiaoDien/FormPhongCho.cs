using System.Drawing;
using System.Drawing.Drawing2D;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_13;

// Bài mở rộng MR6.13 – Trang bán vé concert có phòng chờ ảo (ứng dụng người dùng, WinForms).
//
// Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.cs (FormNguoiDungCoSo); tệp này vẽ
// áp phích concert (GDI+: nền chuyển màu, đèn sân khấu, đám đông) và ba "trang" như một trang bán vé thật:
//   - trang sự kiện: giá vé, vé còn lại, giới thiệu phòng chờ, nút "VÀO HÀNG CHỜ MUA VÉ" (phòng chờ tắt: "MUA VÉ NGAY");
//   - phòng chờ: số thứ tự, thanh tiến trình vị trí, số người phía trước, thời gian chờ ước tính, đang phục vụ tới số
//     mấy (cập nhật trực tiếp từ cap_nhat); yêu cầu cho_luot "chờ dài" – máy chủ trả lời khi tới lượt;
//   - trang mua: "ĐẾN LƯỢT BẠN!", đồng hồ đếm ngược thời hạn lượt (đỏ khi < 5 s), chọn số vé, THANH TOÁN, kết quả.
sealed class FormPhongCho(string ten, string host, int cong) : FormNguoiDungCoSo(ten, host, cong)
{
    static readonly Color Tim = GD.Hex("#3b0a5e"), Hong = GD.Hex("#e0407a");
    protected override string TieuDe => "Bán vé concert";
    protected override Size KichThuocKhung => new(548, 412);

    /// <summary>Áp phích concert: nền chuyển màu, đèn sân khấu, đám đông, tên sự kiện.</summary>
    static void VeApPhich(Graphics g, int w, int h, string ten, string phu = "20:00 · 20.12.2026 · Sân khấu ngoài trời")
    {
        g.SmoothingMode = SmoothingMode.AntiAlias;
        using (var nen = new LinearGradientBrush(new Rectangle(0, 0, w, h), GD.Hex("#12002a"), GD.Hex("#6a1b6e"), LinearGradientMode.Vertical))
            g.FillRectangle(nen, 0, 0, w, h);
        var rnd = new Random(7);
        using (var b = new SolidBrush(GD.Hex("#fff4c0")))
            for (int i = 0; i < 40; i++) g.FillEllipse(b, rnd.Next(0, w + 1), rnd.Next(0, h / 2 + 1), 2, 2);   // sao
        foreach (var (tile, mau) in new[] { (0.18f, "#ffd36e"), (0.5f, "#7fd6ff"), (0.82f, "#ff8fc0") })        // đèn chiếu
        {
            float x0 = w * tile, y0 = h * 0.62f;
            var c = GD.Hex(mau);
            using (var b = new SolidBrush(Color.FromArgb(64, c))) g.FillPolygon(b, new PointF[] { new(x0, y0), new(x0 - 70, -10), new(x0 + 70, -10) });
            using (var b = new SolidBrush(c)) g.FillEllipse(b, x0 - 7, y0 - 7, 14, 14);
        }
        using (var b = new SolidBrush(GD.Hex("#2a0f3a"))) g.FillRectangle(b, 0, h * 0.66f, w, h * 0.06f);       // sân khấu
        using (var dd = new SolidBrush(GD.Hex("#0a0012")))
        using (var tay = new Pen(GD.Hex("#0a0012"), 4))
            for (int i = 0; i <= w / 14; i++)                                                                   // đám đông
            {
                int x = i * 14 + rnd.Next(-3, 4), y = h - 18 + rnd.Next(-4, 5);
                g.FillEllipse(dd, x - 7, y - 7, 14, 14);
                g.FillRectangle(dd, x - 9, y + 4, 18, h - y - 4);
                if (rnd.NextDouble() < 0.3) g.DrawLine(tay, x + 5, y, x + 10, y - 20);
            }
        using var trai = new StringFormat { LineAlignment = StringAlignment.Center };
        using (var f = GD.Chu(18, true)) g.DrawString(ten.ToUpperInvariant(), f, Brushes.White, 20, 30, trai);
        using (var f = GD.Chu(10))
        using (var b = new SolidBrush(GD.Hex("#ffd9f0"))) g.DrawString(phu, f, b, 22, 58, trai);
        using (var b = new SolidBrush(Hong)) g.FillRectangle(b, w - 120, 14, 106, 26);
        using (var f = GD.Chu(10, true))
        using (var giua = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center })
            g.DrawString("MỞ BÁN", f, Brushes.White, w - 67, 27, giua);
    }

    /// <summary>00:15 khi ≥ 10 s, 00:04.3 khi dưới 10 s.</summary>
    static string DongHo(double giay)
    {
        giay = Math.Max(0.0, giay);
        return giay >= 10 ? $"00:{(int)giay:00}" : $"00:{giay.ToString("00.0", Chung.Ic)}";
    }

    public Panel? Trang;                                 // trang đang hiện
    public Panel TrangSk = null!, TrangCho = null!, TrangMua = null!;
    long? so;                                            // số thứ tự trong hàng
    long? truocDau;                                      // số người phía trước lúc vào hàng
    double? han;                                         // hạn lượt (đồng hồ đơn điệu); null: không giới hạn
    bool coLuot;
    public int SoKq, SoDon;
    string tenCu = "";
    OVe apPhich = null!;
    Label lblGia = null!, lblConSk = null!, lblPc = null!, lblKqSk = null!, lblPhucVuCho = null!, lblConCho = null!, lblTien = null!;
    ProgressBar thanhConSk = null!, thanhCho = null!;
    public Button BtnVao = null!, BtnMua = null!, BtnVe = null!;
    public Label LblSo = null!, LblTruoc = null!, LblBangLuot = null!, LblDongHo = null!, LblKq = null!;
    NumericUpDown oSoVe = null!;

    protected override void Ve(Panel khung)
    {
        apPhich = new OVe(4, 4, 540, 150) { Ve = g => VeApPhich(g, 540, 150, tenCu == "" ? "Đêm nhạc" : tenCu) };
        var hop = new Panel { Location = new Point(4, 158), Size = new Size(540, 250), BackColor = Color.White, BorderStyle = BorderStyle.FixedSingle };
        TrangSk = VeSuKienTrang();
        TrangCho = VeChoTrang();
        TrangMua = VeMuaTrang();
        hop.Controls.AddRange([TrangSk, TrangCho, TrangMua]);
        GD.Them(khung, apPhich, hop);
        Sang(TrangSk);
    }

    static Panel MoiTrang() => new() { Location = new Point(0, 0), Size = new Size(538, 248), BackColor = Color.White, Visible = false };

    static Label N(string t, int x, int y, int w, int h, Font f, Color? mau = null) =>
        new() { Text = t, Location = new Point(x, y), Size = new Size(w, h), Font = f, BackColor = Color.White, ForeColor = mau ?? Color.Black };

    // ------------------------------------------------------------ ba trang
    Panel VeSuKienTrang()
    {
        var f = MoiTrang();
        lblGia = N("–", 12, 12, 510, 22, GD.Chu(11, true), Tim);
        lblConSk = N("Vé còn lại: –", 12, 36, 510, 20, GD.Chu(10));
        thanhConSk = new ProgressBar { Location = new Point(12, 58), Size = new Size(510, 16), Maximum = 500 };
        lblPc = N("", 12, 82, 510, 40, GD.Chu(9), GD.Hex("#404040"));
        BtnVao = new Button
        {
            Text = "VÀO HÀNG CHỜ MUA VÉ", Location = new Point(119, 130), Size = new Size(300, 44), BackColor = Hong, ForeColor = Color.White,
            Font = GD.Chu(13, true), FlatStyle = FlatStyle.Flat, UseVisualStyleBackColor = false, Enabled = false,
        };
        BtnVao.Click += (_, _) => VaoHang();
        lblKqSk = N("", 12, 186, 510, 24, GD.Chu(10, true), GD.DoDam);
        lblKqSk.TextAlign = ContentAlignment.MiddleCenter;
        f.Controls.AddRange([lblGia, lblConSk, thanhConSk, lblPc, BtnVao, lblKqSk]);
        return f;
    }

    Panel VeChoTrang()
    {
        var f = MoiTrang();
        var tieuDe = N("Bạn đang ở trong phòng chờ", 12, 12, 510, 26, GD.Chu(13, true), Tim);
        LblSo = N("Số thứ tự của bạn: –", 12, 40, 510, 22, GD.Chu(11));
        thanhCho = new ProgressBar { Location = new Point(12, 66), Size = new Size(510, 18), Maximum = 1000 };
        LblTruoc = N("–", 12, 92, 510, 24, GD.Chu(12, true));
        lblPhucVuCho = N("", 12, 120, 510, 18, GD.Chu(9), GD.Hex("#404040"));
        lblConCho = N("", 12, 140, 510, 18, GD.Chu(9), GD.Hex("#404040"));
        var luuY = N("Vui lòng không tải lại trang – bạn vẫn giữ nguyên chỗ trong hàng.", 12, 172, 510, 22, GD.Chu(9));
        luuY.BackColor = GD.Hex("#fff3c4");
        luuY.TextAlign = ContentAlignment.MiddleLeft;
        f.Controls.AddRange([tieuDe, LblSo, thanhCho, LblTruoc, lblPhucVuCho, lblConCho, luuY]);
        return f;
    }

    Panel VeMuaTrang()
    {
        var f = MoiTrang();
        LblBangLuot = new Label
        {
            Text = "ĐẾN LƯỢT BẠN!", Location = new Point(0, 0), Size = new Size(538, 30), BackColor = GD.Hex("#2e9e4f"), ForeColor = Color.White,
            Font = GD.Chu(12, true), TextAlign = ContentAlignment.MiddleCenter,
        };
        var nhanCon = N("Thời gian còn lại để hoàn tất:", 12, 46, 200, 20, GD.Thuong);
        LblDongHo = N("–", 214, 34, 200, 42, GD.ChuCoDinh(24, true), GD.XanhDam);
        var nhanSoVe = N("Số vé:", 12, 88, 56, 24, GD.Chu(11));
        oSoVe = new NumericUpDown { Location = new Point(70, 86), Size = new Size(50, 26), Minimum = 1, Maximum = 4, Value = 2, Font = GD.Chu(12, true) };
        oSoVe.ValueChanged += (_, _) => TinhTien();
        lblTien = N("", 130, 90, 390, 20, GD.Chu(10));
        BtnMua = new Button
        {
            Text = "THANH TOÁN", Location = new Point(149, 120), Size = new Size(240, 38), BackColor = GD.Hex("#ff8c00"), ForeColor = Color.White,
            Font = GD.Chu(12, true), FlatStyle = FlatStyle.Flat, UseVisualStyleBackColor = false,
        };
        BtnMua.Click += (_, _) => ThanhToan();
        LblKq = N("", 12, 164, 514, 40, GD.Chu(11, true));
        LblKq.TextAlign = ContentAlignment.MiddleCenter;
        BtnVe = GD.Nut("Về trang sự kiện", GD.Hex("#e8eef8"), 194, 208, 150, 28);
        BtnVe.Visible = false;
        BtnVe.Click += (_, _) => VeSuKien();
        f.Controls.AddRange([LblBangLuot, nhanCon, LblDongHo, nhanSoVe, oSoVe, lblTien, BtnMua, LblKq, BtnVe]);
        return f;
    }

    void Sang(Panel trang)
    {
        foreach (var t in new[] { TrangSk, TrangCho, TrangMua }) t.Visible = t == trang;
        Trang = trang;
    }

    // ------------------------------------------------------------ thao tác
    void VaoHang()
    {
        lblKqSk.Text = "";
        if (!Chung.Dung(Tt, "phong_cho"))                // phòng chờ tắt: vào thẳng trang mua
        {
            MoTrangMua(null);
            return;
        }
        BtnVao.Enabled = false;
        NhatKy.Ghi("Vào phòng chờ…");
        GuiYc("xep_hang");
    }

    void MoTrangMua(double? hanMs)
    {
        coLuot = true;
        han = hanMs is { } ms ? Chung.Now + ms / 1000 : null;
        (LblBangLuot.Text, LblBangLuot.BackColor) = hanMs != null ? ("ĐẾN LƯỢT BẠN! Hãy hoàn tất trước khi hết giờ", GD.Hex("#2e9e4f"))
                                                                  : ("MUA TRỰC TIẾP (phòng chờ tắt)", Tim);
        LblKq.Text = "";
        BtnMua.Enabled = true;
        BtnVe.Visible = false;
        TinhTien();
        Sang(TrangMua);
    }

    /// <summary>Chọn số vé rồi bấm THANH TOÁN (kịch bản demo).</summary>
    public void Mua(int n)
    {
        oSoVe.Value = Math.Clamp(n, (int)oSoVe.Minimum, (int)oSoVe.Maximum);
        TinhTien();
        BtnMua.PerformClick();
    }

    void ThanhToan()
    {
        int n = (int)oSoVe.Value;
        NhatKy.Ghi($"Thanh toán {n} vé…");
        (LblKq.Text, LblKq.ForeColor) = ("Đang xử lý thanh toán…", GD.Hex("#404040"));
        GuiYc("mua", new JsonObject { ["so_ve"] = n });
    }

    public void VeSuKien()
    {
        (so, truocDau, han, coLuot) = (null, null, null, false);
        BtnVao.Enabled = Tt != null && !Chung.Dung(Tt, "het_ve");
        Sang(TrangSk);
    }

    void TinhTien()
    {
        if (Tt == null) return;
        long n = (long)oSoVe.Value, gia = Chung.SoL(Tt["su_kien"] as JsonObject, "gia_ve");
        lblTien.Text = $"× {Chung.Tien(gia)} = {Chung.Tien(n * gia)}";
    }

    protected override void KetQua(string loai, JsonObject truong, JsonObject tin, double rtt)
    {
        SoKq++;
        bool ok = Chung.Dung(tin, "ok");
        string lyDo = Chung.ChuoiPy(tin["ly_do"]);
        if (loai == "xep_hang")
        {
            if (!ok)
            {
                NhatKy.Ghi($"  không vào được hàng: {lyDo}");
                lblKqSk.Text = $"Rất tiếc: {lyDo}";
                BtnVao.Enabled = true;
                return;
            }
            (so, truocDau) = (Chung.SoL(tin, "so"), Math.Max(1, Chung.SoL(tin, "truoc")));
            NhatKy.Ghi($"  số thứ tự #{so}, {Chung.SoL(tin, "truoc")} người phía trước, ~{Chung.F(Chung.SoD(tin, "cho_s"), "F1")} s");
            Sang(TrangCho);
            VeViTri();
            GuiYc("cho_luot");                           // chờ dài: máy chủ trả lời khi tới lượt
        }
        else if (loai == "cho_luot")
        {
            if (ok)
            {
                NhatKy.Ghi($"  ĐẾN LƯỢT sau {Chung.F(rtt / 1000, "F1")} s chờ, còn {Chung.F(Chung.SoD(tin, "han_ms") / 1000, "F0")} s để mua");
                MoTrangMua(Chung.SoD(tin, "han_ms"));
            }
            else
            {
                NhatKy.Ghi($"  {lyDo}");
                VeSuKien();
                lblKqSk.Text = $"Rất tiếc: {lyDo}";
            }
        }
        else if (ok)
        {
            SoDon++;
            NhatKy.Ghi($"  MUA THÀNH CÔNG ({Chung.F(rtt, "F1")} ms): đơn #{Chung.SoL(tin, "ma_don")}, {Chung.SoL(tin, "so_ve")} vé");
            (LblKq.Text, LblKq.ForeColor) = ($"Mua thành công {Chung.SoL(tin, "so_ve")} vé – đơn #{Chung.SoL(tin, "ma_don")},"
                                             + $" {Chung.Tien(Chung.SoL(tin, "thanh_tien"))}", GD.XanhDam);
            BtnMua.Enabled = false;
            (han, coLuot) = (null, false);
            (LblDongHo.Text, LblDongHo.ForeColor) = ("✓", GD.XanhDam);
            BtnVe.Visible = true;
        }
        else
        {
            string? ma = Chung.Chuoi(tin, "ma_loi");
            NhatKy.Ghi($"  từ chối ({Chung.F(rtt, "F1")} ms): {lyDo}");
            string text = ma switch
            {
                "qua_tai" => "Hệ thống đang quá tải, vui lòng thử lại",
                "het_ve" => "Rất tiếc, đã bán hết vé",
                "da_mua" => "Lượt này đã được dùng để mua",
                _ => Chung.Chuoi(tin, "ly_do") ?? "",
            };
            (LblKq.Text, LblKq.ForeColor) = (text, GD.DoDam);
            if (ma is "het_ve" or "het_luot" or "da_mua")
            {
                BtnMua.Enabled = false;
                BtnVe.Visible = true;
            }
        }
    }

    void VeViTri()
    {
        if (so is not { } s || Tt is not { } tt) return;
        long phucVu = Chung.SoL(tt, "dang_phuc_vu"), truoc = Math.Max(0, s - phucVu - 1);
        double tocDo = Chung.SoD(tt, "toc_do", 1);
        LblSo.Text = $"Số thứ tự của bạn: #{s}";
        thanhCho.Value = (int)Math.Clamp(1000 * (1 - (double)truoc / Math.Max(truocDau ?? 1, 1)), 0, 1000);
        LblTruoc.Text = truoc > 0 ? $"Còn khoảng {truoc} người phía trước – chờ ~{Chung.F((truoc + 1) / tocDo, "F0")} s" : "Sắp tới lượt bạn…";
        lblPhucVuCho.Text = $"Đang phục vụ tới số #{phucVu} · cấp lượt {Chung.G(tocDo)} người/s · {Chung.SoL(tt, "dang_cho")} người đang chờ";
        lblConCho.Text = $"Vé còn lại: {Chung.SoL(tt, "con_lai")} / {Chung.SoL(tt["su_kien"] as JsonObject, "tong_ve")}";
    }

    // ------------------------------------------------------------ nhận trạng thái
    protected override void CapNhat(JsonObject tt)
    {
        var sk = tt["su_kien"] as JsonObject;
        string ten = Chung.Chuoi(sk, "ten") ?? "";
        if (ten != tenCu)
        {
            tenCu = ten;
            apPhich.Invalidate();
        }
        long tongVe = Chung.SoL(sk, "tong_ve"), con = Chung.SoL(tt, "con_lai");
        bool hetVe = Chung.Dung(tt, "het_ve"), pc = Chung.Dung(tt, "phong_cho");
        lblGia.Text = $"Giá vé {Chung.Tien(Chung.SoL(sk, "gia_ve"))} · tối đa {Chung.SoL(sk, "ve_toi_da")} vé/người";
        lblConSk.Text = $"Vé còn lại: {con} / {tongVe}" + (hetVe ? "  – ĐÃ BÁN HẾT" : "");
        thanhConSk.Maximum = (int)Math.Max(1, tongVe);
        thanhConSk.Value = (int)Math.Clamp(con, 0, thanhConSk.Maximum);
        lblPc.Text = pc ? $"Phòng chờ ảo BẬT: mọi người được xếp hàng theo thứ tự đến, mỗi giây cấp lượt cho {Chung.G(Chung.SoD(tt, "toc_do"))}"
                          + $" người; mỗi lượt có {Chung.G(Chung.SoD(sk, "luot_s"))} s để mua."
                        : "Phòng chờ TẮT: ai cũng vào mua trực tiếp cùng lúc.";
        BtnVao.Text = pc ? "VÀO HÀNG CHỜ MUA VÉ" : "MUA VÉ NGAY";
        oSoVe.Maximum = Math.Max(1, Chung.SoL(sk, "ve_toi_da"));
        if (Trang == TrangSk) BtnVao.Enabled = !hetVe;
        VeViTri();
    }

    protected override void MoiNhip()
    {
        if (Trang != TrangMua || !coLuot) return;
        if (han is not { } h)
        {
            (LblDongHo.Text, LblDongHo.ForeColor) = ("∞", GD.XanhDam);
            return;
        }
        double con = h - Chung.Now;
        (LblDongHo.Text, LblDongHo.ForeColor) = (DongHo(con), con < 5 ? GD.DoDam : GD.XanhDam);
        if (con > 0) return;
        coLuot = false;
        BtnMua.Enabled = false;
        (LblKq.Text, LblKq.ForeColor) = ("Hết thời gian giữ lượt – bạn cần xếp hàng lại", GD.DoDam);
        BtnVe.Visible = true;
        NhatKy.Ghi("Lượt mua đã hết hạn");
    }

    protected override string DongDinhKy()
    {
        var tt = Tt!;
        string trang = Trang == TrangCho ? "phòng chờ" : Trang == TrangMua ? "mua" : "sự kiện";
        string viTri = so is { } s ? $", số #{s}, phục vụ #{Chung.SoL(tt, "dang_phuc_vu")}" : "";
        return $"trang {trang}{viTri}; vé còn {Chung.SoL(tt, "con_lai")}; đơn của bạn {SoDon}";
    }
}
