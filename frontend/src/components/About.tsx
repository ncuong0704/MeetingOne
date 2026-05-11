import React from "react";
import Image from 'next/image';
import { BRAND_NAME, BRAND_LOGO_PATH } from '@/constants/brand';
import { Lock, Cpu, Banknote, Globe, Github, Shield } from 'lucide-react';

const features = [
  {
    icon: Lock,
    title: 'Ưu tiên quyền riêng tư',
    desc: 'Dữ liệu và quy trình xử lý AI giữ trong phạm vi của bạn — không phụ thuộc đám mây.',
    color: 'text-blue-600',
    bg: 'bg-blue-50',
  },
  {
    icon: Cpu,
    title: 'Linh hoạt mô hình',
    desc: 'Dùng mô hình mã nguồn mở cục bộ hay API bên ngoài đều được — không bị khóa nhà cung cấp.',
    color: 'text-violet-600',
    bg: 'bg-violet-50',
  },
  {
    icon: Banknote,
    title: 'Tiết kiệm chi phí',
    desc: 'Giảm chi phí bằng cách chạy mô hình cục bộ hoặc chỉ trả cho các lần gọi bạn chọn.',
    color: 'text-emerald-600',
    bg: 'bg-emerald-50',
  },
  {
    icon: Globe,
    title: 'Làm việc mọi nơi',
    desc: 'Google Meet, Zoom, Teams — trực tuyến hay ngoại tuyến đều hoạt động.',
    color: 'text-orange-600',
    bg: 'bg-orange-50',
  },
];

export function About() {
  return (
    <div className="flex flex-col h-[80vh] overflow-y-auto bg-gray-50">

      {/* ── Hero ──────────────────────────────────────────────────────── */}
      <div className="bg-white border-b border-gray-100 px-6 pb-8 text-center space-y-4">
        <Image
          src={BRAND_LOGO_PATH}
          alt={BRAND_NAME}
          width={100}
          height={100}
          className="mx-auto object-contain"
        />

        <div className="space-y-1">
          <h1 className="text-2xl font-bold text-gray-900">{BRAND_NAME}</h1>
          <p className="text-sm text-gray-500 max-w-sm mx-auto leading-relaxed">
            Ghi chú và tóm tắt thời gian thực — dữ liệu không rời khỏi máy của bạn.
          </p>
        </div>

        {/* Badges */}
        <div className="flex items-center justify-center gap-2 flex-wrap">
          <span className="inline-flex items-center gap-1.5 rounded-full bg-blue-50 px-3 py-1 text-xs font-medium text-blue-600">
            <Shield className="w-3 h-3" />
            Ưu tiên quyền riêng tư
          </span>
          <span className="inline-flex items-center gap-1.5 rounded-full bg-emerald-50 px-3 py-1 text-xs font-medium text-emerald-600">
            <Cpu className="w-3 h-3" />
            Sẵn sàng ngoại tuyến
          </span>
        </div>
      </div>

      {/* ── Features ──────────────────────────────────────────────────── */}
      <div className="px-5 py-5 space-y-2.5">
        <p className="text-[11px] font-semibold uppercase tracking-widest text-gray-400 px-1">Tính năng nổi bật</p>
        <div className="grid grid-cols-2 gap-2.5">
          {features.map(({ icon: Icon, title, desc, color, bg }) => (
            <div
              key={title}
              className="bg-white rounded-xl border border-gray-100 p-3.5 shadow-sm hover:shadow-md hover:border-gray-200 transition-all"
            >
              <div className={`w-7 h-7 rounded-lg ${bg} flex items-center justify-center mb-2.5`}>
                <Icon className={`w-3.5 h-3.5 ${color}`} />
              </div>
              <h3 className="text-xs font-semibold text-gray-800 mb-1 leading-snug">{title}</h3>
              <p className="text-[11px] text-gray-500 leading-relaxed">{desc}</p>
            </div>
          ))}
        </div>
      </div>

    </div>
  );
}
