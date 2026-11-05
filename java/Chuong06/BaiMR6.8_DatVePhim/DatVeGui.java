import java.awt.BasicStroke;
import java.awt.Color;
import java.awt.Dimension;
import java.awt.Font;
import java.awt.Graphics2D;
import java.awt.event.MouseAdapter;
import java.awt.event.MouseEvent;
import java.awt.geom.Arc2D;
import java.awt.geom.Ellipse2D;
import java.awt.geom.Line2D;
import java.awt.geom.QuadCurve2D;
import java.awt.geom.Rectangle2D;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeSet;
import javax.swing.BorderFactory;
import javax.swing.Box;
import javax.swing.BoxLayout;
import javax.swing.JButton;
import javax.swing.JComponent;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.SwingUtilities;

/**
 * Bài mở rộng MR6.8 – Ứng dụng đặt vé xem phim (người dùng, Swing).
 *
 * Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.CuaSoNguoiDungCoSo; tệp này vẽ ứng
 * dụng đặt vé: áp phích phim (Graphics2D), sơ đồ phòng chiếu 8 × 12 ghế với MÀN HÌNH, lối đi, nhãn hàng A–H; màu ghế:
 * trống / đang chọn / người khác giữ / đã bán / bạn giữ / vé của bạn. Bấm ghế để chọn, "GIỮ CHỖ" gửi giu_cho, đồng hồ
 * giữ chỗ đếm ngược (đỏ khi < 10 s), "THANH TOÁN" và "Nhả ghế". Sơ đồ cập nhật trực tiếp từ cap_nhat (≤ 10 lần/giây)
 * khi người khác giữ/mua ghế; ghế đang chọn bị người khác giữ thì tự bỏ chọn và báo.
 *
 * Chạy:  java -cp out DatVeGui <tên> [host] [cổng]       (mặc định 127.0.0.1 43722)
 */
public class DatVeGui extends GiaoDienChung.CuaSoNguoiDungCoSo {
    static final Color TRONG = new Color(0xffffff), CHON = new Color(0xb9f6ca), KHAC_GIU = new Color(0xffa726),
            DA_BAN = new Color(0x9e9e9e), TOI_GIU = new Color(0x2e7d32), VE_TOI = new Color(0x1565c0);
    static final Color DO_RAP = new Color(0xe71a0f);
    static final Object[][] CHU_GIAI = {{TRONG, "Trống"}, {CHON, "Đang chọn"}, {KHAC_GIU, "Người khác giữ"}, {DA_BAN, "Đã bán"},
            {TOI_GIU, "Bạn giữ"}, {VE_TOI, "Vé của bạn"}};

    static String dongHo(double giay) {
        int s = Math.max(0, (int) (giay + 0.999));
        return Chung.f("%02d:%02d", s / 60, s % 60);
    }

    /** Vẽ sơ đồ ghế; dùng ở ứng dụng đặt vé (ô = 30 px) và bảng điều khiển máy chủ (ô nhỏ). */
    static final class SoDoGhe {
        final String hang;
        final int cot;
        final double o, khe, le, tren;
        final int[] loiDi;
        final boolean chu;
        final double rong, cao;
        final Map<String, double[]> viTri = new LinkedHashMap<>();

        SoDoGhe(String hang, int cot, double o, double khe, double le, double tren, int[] loiDi, boolean chu) {
            this.hang = hang;
            this.cot = cot;
            this.o = o;
            this.khe = khe;
            this.le = le;
            this.tren = tren;
            this.loiDi = loiDi;
            this.chu = chu;
            rong = le * 2 + cot * (o + khe) + loiDi.length * o * 0.5;
            cao = tren + hang.length() * (o * 0.86 + khe) + 4;
            for (int i = 0; i < hang.length(); i++) {
                double y = tren + i * (o * 0.86 + khe);
                for (int c = 1; c <= cot; c++) {
                    double x = le + (c - 1) * (o + khe);
                    for (int a : loiDi) if (c > a) x += o * 0.5;
                    viTri.put("" + hang.charAt(i) + c, new double[] {x, y, x + o, y + o * 0.86});
                }
            }
        }

        SoDoGhe(String hang, int cot) { this(hang, cot, 30, 4, 24, 46, new int[] {2, 10}, true); }

        void veNen(Graphics2D g) {
            g.setColor(new Color(0x1c1c24));
            g.fill(new Rectangle2D.Double(0, 0, rong + 40, cao + 200));
            double lech = tren * 0.55;
            g.setColor(new Color(0xe8e8f0));
            g.setStroke(new BasicStroke(chu ? 4 : 2));
            g.draw(new QuadCurve2D.Double(le, lech, rong * 0.5, lech * 0.35, rong - le, lech));
            g.setStroke(new BasicStroke(1));
            if (chu) {
                g.setColor(new Color(0xb0b0c0));
                g.setFont(new Font(Font.DIALOG, Font.BOLD, 10));
                GiaoDienChung.chuGiua(g, "MÀN HÌNH", rong / 2, lech + 10);
                g.setColor(new Color(0xc0c0d0));
                g.setFont(new Font(Font.DIALOG, Font.BOLD, 12));
                for (int i = 0; i < hang.length(); i++) {
                    double[] r = viTri.get(hang.charAt(i) + "1");
                    double y = (r[1] + r[3]) / 2;
                    GiaoDienChung.chuGiua(g, "" + hang.charAt(i), le / 2, y);
                    GiaoDienChung.chuGiua(g, "" + hang.charAt(i), rong - le / 2, y);
                }
            }
        }

        /** mauGhe: {ghế: màu}; vien: {ghế: màu viền} (ghế nóng ở máy chủ). */
        void ve(Graphics2D g, Map<String, Color> mauGhe, Map<String, Color> vien) {
            g.setFont(new Font(Font.DIALOG, Font.PLAIN, 9));
            for (Map.Entry<String, double[]> e : viTri.entrySet()) {
                String ghe = e.getKey();
                double[] r = e.getValue();
                Color mau = mauGhe.getOrDefault(ghe, TRONG);
                Rectangle2D o2 = new Rectangle2D.Double(r[0], r[1], r[2] - r[0], r[3] - r[1]);
                g.setColor(mau);
                g.fill(o2);
                boolean nong = vien != null && vien.containsKey(ghe);
                g.setColor(nong ? vien.get(ghe) : new Color(0x606070));
                g.setStroke(new BasicStroke(nong ? 2 : 1));
                g.draw(o2);
                g.setStroke(new BasicStroke(1));
                if (chu) {                                             // tay vịn: vạch tối ở chân ghế
                    g.setColor(new Color(0x707080));
                    g.fill(new Rectangle2D.Double(r[0] + 2, r[3] - 4, r[2] - r[0] - 4, 3));
                    boolean toi = mau.equals(TOI_GIU) || mau.equals(VE_TOI) || mau.equals(DA_BAN);
                    g.setColor(toi ? Color.WHITE : new Color(0x303040));
                    GiaoDienChung.chuGiua(g, ghe.substring(1), (r[0] + r[2]) / 2, (r[1] + r[3]) / 2 - 2);
                    if (mau.equals(DA_BAN)) {
                        g.setColor(new Color(0xe0e0e0));
                        g.draw(new Line2D.Double(r[0] + 4, r[1] + 3, r[2] - 4, r[3] - 6));
                    }
                }
            }
        }

        String gheTai(double x, double y) {
            for (Map.Entry<String, double[]> e : viTri.entrySet()) {
                double[] r = e.getValue();
                if (r[0] <= x && x <= r[2] && r[1] <= y && y <= r[3]) return e.getKey();
            }
            return null;
        }
    }

    // ------------------------------------------------------------ trạng thái cửa sổ
    TreeSet<String> chon, giu, ve;     // ghế đang chọn (chưa giữ); ghế mình đang giữ (theo kq); vé của mình
    long phienGiu, phienVe;            // số thứ tự lần ghi (kq) của lần giữ chỗ / thanh toán gần nhất
    Double han;                        // thời điểm (đồng hồ đơn điệu) hết hạn giữ chỗ
    int soGiu, soVe;
    String phimCu;
    SoDoGhe soDo;
    JComponent cv, cvPhim;
    JLabel nhanGhe, nhanTien, nhanDongHo, thongBao, nhanVe;
    JButton nutGiu, nutTt, nutNha;

    DatVeGui(String ten, String host, int cong) {
        super("Đặt vé xem phim", ten, host, cong);
        xay();
    }

    static JLabel dong(String chu, Font f, Color mau) {
        JLabel l = GiaoDienChung.nhan(chu, f, mau);
        l.setMaximumSize(new Dimension(Integer.MAX_VALUE, 200));
        return l;
    }

    static JButton nutLon(String chu, Color mau) {
        JButton b = GiaoDienChung.nut(chu, mau, 0);
        b.setForeground(Color.WHITE);
        b.setFont(GiaoDienChung.chu(14, true));
        b.setBorderPainted(false);
        b.setAlignmentX(0);
        b.setMaximumSize(new Dimension(Integer.MAX_VALUE, 32));
        return b;
    }

    @Override
    protected void ve(JPanel khung) {
        chon = new TreeSet<>();
        giu = new TreeSet<>();
        ve = new TreeSet<>();
        khung.setLayout(new BoxLayout(khung, BoxLayout.X_AXIS));
        soDo = new SoDoGhe("ABCDEFGH", 12);
        cv = new GiaoDienChung.KhungVe(486, 322, this::veGhe);
        cv.addMouseListener(new MouseAdapter() {
            @Override public void mousePressed(MouseEvent e) {
                String g = soDo.gheTai(e.getX(), e.getY());
                if (g != null) bam(g);
            }
        });
        JPanel trai = GiaoDienChung.cot();
        trai.add(Box.createVerticalStrut(4));
        trai.add(cv);
        khung.add(trai);
        khung.add(Box.createHorizontalStrut(8));

        JPanel phai = new JPanel();
        phai.setLayout(new BoxLayout(phai, BoxLayout.Y_AXIS));
        phai.setBackground(Color.WHITE);
        phai.setBorder(BorderFactory.createCompoundBorder(BorderFactory.createLineBorder(Color.BLACK),
                BorderFactory.createEmptyBorder(6, 6, 6, 6)));
        cvPhim = new GiaoDienChung.KhungVe(250, 96, this::vePhim);
        phai.add(cvPhim);
        phai.add(Box.createVerticalStrut(4));
        nhanGhe = dong("Ghế đã chọn: –", GiaoDienChung.chu(13, true), Color.BLACK);
        nhanTien = dong("Tạm tính: 0 đ", GiaoDienChung.chu(12, false), Color.BLACK);
        phai.add(nhanGhe);
        phai.add(nhanTien);
        phai.add(Box.createVerticalStrut(4));
        nutGiu = nutLon("GIỮ CHỖ", new Color(0xff8f00));
        nutGiu.addActionListener(e -> giuCho());
        phai.add(nutGiu);
        phai.add(Box.createVerticalStrut(2));
        nhanDongHo = GiaoDienChung.nhan("--:--", GiaoDienChung.chuDeu(24, true), new Color(0x909090));
        JPanel f = GiaoDienChung.hang(GiaoDienChung.nhan("Giữ chỗ còn", GiaoDienChung.chu(12, false), null), nhanDongHo);
        f.setBackground(Color.WHITE);
        phai.add(f);
        nutTt = nutLon("THANH TOÁN", DO_RAP);
        nutTt.addActionListener(e -> thanhToan());
        phai.add(nutTt);
        phai.add(Box.createVerticalStrut(2));
        nutNha = GiaoDienChung.nut("Nhả ghế", new Color(0xf0f0f0), 0);
        nutNha.addActionListener(e -> nha());
        phai.add(nutNha);
        phai.add(Box.createVerticalStrut(2));
        thongBao = dong("Chưa kết nối", GiaoDienChung.chu(12, true), Color.BLACK);
        thongBao.setOpaque(true);
        thongBao.setBackground(new Color(0xe8e8e8));
        thongBao.setHorizontalAlignment(JLabel.CENTER);
        thongBao.setBorder(BorderFactory.createEmptyBorder(4, 4, 4, 4));
        phai.add(thongBao);
        phai.add(Box.createVerticalStrut(2));
        nhanVe = dong("Vé của bạn: –", GiaoDienChung.chu(12, false), VE_TOI);
        phai.add(nhanVe);
        phai.add(Box.createVerticalGlue());
        JPanel boc = GiaoDienChung.cot();
        boc.add(Box.createVerticalStrut(4));
        boc.add(phai);
        khung.add(boc);
        bao("Chưa kết nối", new Color(0xe8e8e8), Color.BLACK);          // nạp sẵn bộ hiển thị HTML của Swing
        veLaiVe();
        batNut();
    }

    // ------------------------------------------------------------ áp phích, sơ đồ
    void vePhim(Graphics2D g) {
        for (int i = 0; i < 12; i++) {                                // nền chuyển màu tím -> xanh
            g.setColor(new Color(43 + i * 2, 22 + i * 6, 64 + i * 9));
            g.fillRect(0, i * 8, 250, 8);
        }
        if (tt == null) return;
        Ellipse2D tron = new Ellipse2D.Double(196, 8, 46, 46);
        g.setColor(new Color(0x4fc3f7));
        g.fill(tron);
        g.setColor(new Color(0x43a047));
        g.fill(new Arc2D.Double(196, 8, 46, 46, 200, 140, Arc2D.PIE));
        g.setColor(new Color(0xb3e5fc));
        g.setStroke(new BasicStroke(2));
        g.draw(tron);
        g.setColor(Color.WHITE);
        for (int k = 0; k < 14; k++) g.fill(new Ellipse2D.Double((k * 37) % 170 + 6, (k * 23) % 60 + 6, 2, 2));
        g.setFont(GiaoDienChung.chu(16, true));
        var fm = g.getFontMetrics();
        g.drawString(String.valueOf(tt.get("phim")).toUpperCase(), 10, 40 + fm.getAscent() / 2 - 2);
        g.setColor(new Color(0xe0d8f0));
        g.setFont(GiaoDienChung.chu(11, false));
        g.drawString("2D Phụ đề · Suất " + tt.get("suat") + " · Phòng 3", 10, 70);
        g.setColor(new Color(0xffd54f));
        g.setFont(GiaoDienChung.chu(11, true));
        g.drawString(Chung.tien(Chung.so(tt.get("gia_ve"))) + "/ghế · giữ chỗ " + Chung.g(Chung.sod(tt.get("ttl_s"))) + " s", 10, 86);
    }

    void veGhe(Graphics2D g) {
        soDo.veNen(g);
        Map<String, Color> mau = new HashMap<>();
        if (tt != null) {
            String hang = String.valueOf(tt.get("hang"));
            List<Object> sd = Chung.list(tt.get("so_do"));
            Map<String, Object> chu = Chung.map(tt.get("chu"));
            for (int r = 0; r < hang.length(); r++) {
                String d = String.valueOf(sd.get(r));
                for (int c = 0; c < d.length(); c++) {
                    String gh = "" + hang.charAt(r) + (c + 1);
                    boolean cuaToi = ten.equals(chu.get(gh));
                    if (d.charAt(c) == 'b') mau.put(gh, cuaToi ? VE_TOI : DA_BAN);
                    else if (d.charAt(c) == 'g') mau.put(gh, cuaToi ? TOI_GIU : KHAC_GIU);
                }
            }
        }
        for (String gh : chon) mau.putIfAbsent(gh, CHON);
        soDo.ve(g, mau, null);
        double y = soDo.cao + 14, x = 10;
        g.setFont(GiaoDienChung.chu(11, false));
        for (Object[] cg : CHU_GIAI) {                              // chú giải màu
            String chu = (String) cg[1];
            g.setColor((Color) cg[0]);
            g.fill(new Rectangle2D.Double(x, y, 14, 12));
            g.setColor(new Color(0x606070));
            g.draw(new Rectangle2D.Double(x, y, 14, 12));
            g.setColor(new Color(0xd0d0e0));
            g.drawString(chu, (float) (x + 18), (float) (y + 11));
            x += 22 + g.getFontMetrics().stringWidth(chu) + 4;
        }
    }

    /** 'trong' | 'giu' (người khác) | 'ban' | 'toi_giu' | 've' theo cap_nhat mới nhất. */
    String trangGhe(String g) {
        if (tt == null) return "trong";
        int r = String.valueOf(tt.get("hang")).indexOf(g.charAt(0)), c = Integer.parseInt(g.substring(1));
        char x = String.valueOf(Chung.list(tt.get("so_do")).get(r)).charAt(c - 1);
        boolean cuaToi = ten.equals(Chung.map(tt.get("chu")).get(g));
        return x == '.' ? "trong" : x == 'g' ? (cuaToi ? "toi_giu" : "giu") : (cuaToi ? "ve" : "ban");
    }

    /** Bấm một ghế (người dùng hoặc kịch bản demo). */
    void bam(String g) {
        if (kn == null) return;
        String trang = trangGhe(g);
        if (chon.contains(g)) chon.remove(g);
        else if (trang.equals("trong") && !giu.contains(g)) {
            if (chon.size() + giu.size() >= 8) {
                bao("Tối đa 8 ghế mỗi lần", new Color(0xffe0b2), Color.BLACK);
                return;
            }
            chon.add(g);
        } else if (trang.equals("giu")) bao("Ghế " + g + " đang có người giữ", new Color(0xffe0b2), Color.BLACK);
        else if (trang.equals("ban")) bao("Ghế " + g + " đã bán", new Color(0xe0e0e0), Color.BLACK);
        capNhatGio();
    }

    void capNhatGio() {
        long gia = tt != null ? Chung.so(tt.get("gia_ve")) : 0;
        if (!giu.isEmpty()) {
            nhanGhe.setText("Đang giữ: " + String.join(", ", giu));
            nhanTien.setText("Tổng: " + Chung.tien(gia * giu.size()));
            nutTt.setText("THANH TOÁN " + Chung.tien(gia * giu.size()));
        } else {
            nhanGhe.setText("Ghế đã chọn: " + (chon.isEmpty() ? "–" : String.join(", ", chon)));
            nhanTien.setText("Tạm tính: " + Chung.tien(gia * chon.size()));
            nutTt.setText("THANH TOÁN");
        }
        batNut();
        cv.repaint();
    }

    void batNut() {
        boolean co = kn != null, conHan = han != null && Chung.bayGio() < han;
        boolean g = co && !chon.isEmpty() && giu.isEmpty(), t = co && !giu.isEmpty() && conHan;
        nutGiu.setEnabled(g);
        nutGiu.setBackground(g ? new Color(0xff8f00) : new Color(0xffcc80));
        nutTt.setEnabled(t);
        nutTt.setBackground(t ? DO_RAP : new Color(0xef9a9a));
        nutNha.setEnabled(co && !giu.isEmpty());
    }

    void bao(String text, Color nen, Color chu) {
        thongBao.setText(GiaoDienChung.html(text).replace("<html>", "<html><body style='width:220px;text-align:center'>")
                .replace("</html>", "</body></html>"));
        thongBao.putClientProperty("chu", text);
        thongBao.setBackground(nen);
        thongBao.setForeground(chu);
    }

    String thongBaoText() {
        Object c = thongBao.getClientProperty("chu");
        return c == null ? thongBao.getText() : c.toString();
    }

    // ------------------------------------------------------------ giữ / thanh toán / nhả
    void giuCho() {
        if (chon.isEmpty() || !giu.isEmpty()) return;
        List<Object> ghe = new ArrayList<>(chon);
        guiYc("giu_cho", Json.obj("ghe", ghe));                     // gửi trước, vẽ sau
        nhatKy.ghi("Giữ chỗ " + String.join(", ", chon) + "…");
        bao("Đang giữ chỗ…", new Color(0xfff3c4), Color.BLACK);
        nutGiu.setEnabled(false);
    }

    void thanhToan() {
        if (!giu.isEmpty()) {
            guiYc("thanh_toan", Json.obj("ghe", new ArrayList<Object>(giu)));
            nhatKy.ghi("Thanh toán " + String.join(", ", giu) + "…");
        }
    }

    void nha() {
        if (!giu.isEmpty()) guiYc("nha", Json.obj("ghe", new ArrayList<Object>(giu)));
    }

    static List<String> chuoi(Object ds) {
        List<String> p = new ArrayList<>();
        if (ds instanceof List<?> l) for (Object o : l) p.add(String.valueOf(o));
        return p;
    }

    @Override
    protected void ketQua(String loai, Map<String, Object> truong, Map<String, Object> tin, double rtt) {
        List<String> ghe = chuoi(truong.get("ghe"));
        String noi = String.join(", ", ghe);
        if (loai.equals("giu_cho")) {
            if (Chung.dung(tin.get("ok"))) {
                soGiu++;
                giu = new TreeSet<>(chuoi(tin.get("ghe")));
                phienGiu = tin.containsKey("phien") ? Chung.so(tin.get("phien")) : 0;
                chon.removeAll(giu);
                han = Chung.bayGio() + Chung.sod(tin.get("het_han_ms")) / 1000;
                bao("Đã giữ " + noi + Chung.f(" – thanh toán trong %.0f s", Chung.sod(tin.get("het_han_ms")) / 1000),
                        new Color(0xc8f0d2), GiaoDienChung.XANH_DAM);
                nhatKy.ghi("  ĐÃ GIỮ " + noi + Chung.f(" (%.1f ms)", rtt));
                kiemGhe(tt);
            } else {
                chon.removeAll(tin.containsKey("ghe_ban") ? chuoi(tin.get("ghe_ban")) : ghe);
                bao("Không giữ được: " + tin.get("ly_do"), new Color(0xffd0d0), GiaoDienChung.DO_DAM);
                nhatKy.ghi("  không giữ được: " + tin.get("ly_do"));
            }
        } else if (loai.equals("thanh_toan")) {
            if (Chung.dung(tin.get("ok"))) {
                soVe += ghe.size();
                ve.addAll(ghe);
                phienVe = Chung.so(tin.get("phien"));
                giu.removeAll(ghe);
                han = null;
                bao("Đặt vé thành công! " + Chung.tien(Chung.so(tin.get("tien"))), new Color(0xc8f0d2), GiaoDienChung.XANH_DAM);
                nhatKy.ghi("  ĐẶT VÉ THÀNH CÔNG: " + String.join(", ", chuoi(tin.get("ma_ve"))) + Chung.f(" (%.1f ms)", rtt));
            } else {
                bao("Thanh toán bị từ chối: " + tin.get("ly_do"), new Color(0xff6060), Color.WHITE);
                nhatKy.ghi("  thanh toán bị từ chối: " + tin.get("ly_do"));
            }
        } else {
            if (Chung.dung(tin.get("ok"))) {
                List<String> da = chuoi(tin.get("ghe"));
                giu.removeAll(da);
                if (giu.isEmpty()) han = null;
                bao("Đã nhả " + String.join(", ", da), new Color(0xe8e8e8), Color.BLACK);
                nhatKy.ghi("  đã nhả " + String.join(", ", da));
            } else nhatKy.ghi("  nhả bị từ chối: " + tin.get("ly_do"));
        }
        veLaiVe();
        capNhatGio();
    }

    void veLaiVe() {
        nhanVe.setText(GiaoDienChung.html("Vé của bạn: " + (ve.isEmpty() ? "–" : String.join(", ", ve))
                + "\nBạn: giữ được " + soGiu + " lần, mua " + soVe + " vé"));
    }

    /** Ghế mình giữ/mua bị người khác lấy (tranh chấp) hoặc hết hạn; ghế đang chọn bị người khác giữ. Chỉ tin cap_nhat
     *  chụp SAU lần ghi của mình (tt["phien"] ≥ phien của kq giữ chỗ / thanh toán). */
    void kiemGhe(Map<String, Object> t) {
        if (t == null) return;
        Map<String, Object> chu = Chung.map(t.get("chu"));
        long ph = Chung.so(t.get("phien"));
        if (ph >= phienGiu) {
            for (String g : new ArrayList<>(giu)) {
                Object ai = chu.get(g);
                if (ten.equals(ai)) continue;
                giu.remove(g);
                if (ai != null) {
                    bao("Ghế " + g + " đã bị " + ai + " lấy mất!", new Color(0xff6060), Color.WHITE);
                    nhatKy.ghi("Ghế " + g + " đã bị " + ai + " lấy mất!");
                } else {
                    bao("Hết thời gian giữ chỗ – ghế đã được nhả", new Color(0xffd0d0), GiaoDienChung.DO_DAM);
                    nhatKy.ghi("Hết thời gian giữ chỗ: " + g);
                }
            }
            if (giu.isEmpty()) han = null;
        }
        if (ph >= phienVe) {
            for (String g : new ArrayList<>(ve)) {
                Object ai = chu.get(g);
                if (!ten.equals(ai)) {
                    ve.remove(g);
                    String ai2 = ai == null ? "không ai" : String.valueOf(ai);
                    bao("Vé " + g + " bị ghi cho " + ai2 + "!", new Color(0xff6060), Color.WHITE);
                    nhatKy.ghi("Vé " + g + " bị ghi cho " + ai2 + "!");
                }
            }
        }
        List<String> biLay = new ArrayList<>();
        for (String g : chon) if (!trangGhe(g).equals("trong")) biLay.add(g);
        chon.removeAll(biLay);
        if (!biLay.isEmpty()) {
            bao("Ghế " + String.join(", ", biLay) + " vừa có người giữ", new Color(0xffe0b2), Color.BLACK);
            nhatKy.ghi("Ghế " + String.join(", ", biLay) + " vừa có người giữ");
        }
    }

    // ------------------------------------------------------------ nhận trạng thái
    @Override
    protected void capNhat(Map<String, Object> t) {
        String phim = t.get("phim") + "|" + t.get("ttl_s") + "|" + t.get("gia_ve");
        if (!phim.equals(phimCu)) {
            phimCu = phim;
            cvPhim.repaint();
        }
        if (thongBaoText().equals("Chưa kết nối")) bao("Chọn ghế rồi bấm GIỮ CHỖ", new Color(0xe8e8e8), Color.BLACK);
        Map<String, Object> chu = Chung.map(t.get("chu")), dem = Chung.map(t.get("dem"));
        boolean coAi = false;
        for (Object v : chu.values()) if (v != null && !"".equals(v)) coAi = true;
        if (!coAi && (!ve.isEmpty() || !giu.isEmpty()) && Chung.so(dem.get("ban")) == 0 && Chung.so(dem.get("giu")) == 0)
            ve.clear();                                              // máy chủ đặt lại: rạp trống
        Map<String, Object> conLai = Chung.map(t.get("con_lai_ms"));
        Double min = null;
        for (String g : giu)
            if (ten.equals(chu.get(g)) && conLai.containsKey(g)) {
                double x = Chung.sod(conLai.get(g));
                min = min == null ? x : Math.min(min, x);
            }
        if (min != null) han = Chung.bayGio() + min / 1000;
        kiemGhe(t);
        veLaiVe();
        capNhatGio();
    }

    @Override
    protected void moiNhip() {
        if (han == null) {
            nhanDongHo.setText("--:--");
            nhanDongHo.setForeground(new Color(0x909090));
            return;
        }
        double con = han - Chung.bayGio();
        nhanDongHo.setText(dongHo(con));
        nhanDongHo.setForeground(con < 10 ? GiaoDienChung.DO_DAM : GiaoDienChung.XANH_DAM);
        if (con <= 0 && nutTt.isEnabled()) {
            batNut();
            bao("Hết thời gian giữ chỗ", new Color(0xffd0d0), GiaoDienChung.DO_DAM);
        }
    }

    @Override
    protected String dongDinhKy() {
        Map<String, Object> d = Chung.map(tt.get("dem"));
        String con = han == null ? "–" : Chung.f("%.0f s", Math.max(0, han - Chung.bayGio()));
        return "trống " + d.get("trong") + ", giữ " + d.get("giu") + ", bán " + d.get("ban") + "; bạn giữ "
                + (giu.isEmpty() ? "–" : String.join(", ", giu)) + " (còn " + con + "), vé " + (ve.isEmpty() ? "–" : String.join(", ", ve));
    }

    public static void main(String[] args) {
        GiaoDienChung.NhatKy.inRa = true;                            // chạy riêng: nhật ký cũng ra dòng lệnh
        String ten = args.length > 0 ? args[0] : "An";
        String host = args.length > 1 ? args[1] : "127.0.0.1";
        int cong = args.length > 2 ? Integer.parseInt(args[2]) : 43722;
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new DatVeGui(ten, host, cong).frame.setVisible(true);
        });
    }
}
