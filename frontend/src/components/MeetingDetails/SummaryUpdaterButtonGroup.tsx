"use client";

import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Save, Loader2, Download, FileText, FileDown, ChevronDown } from 'lucide-react';
import Analytics from '@/lib/analytics';

interface SummaryUpdaterButtonGroupProps {
  isSaving: boolean;
  isDirty: boolean;
  onSave: () => Promise<void>;
  onFind?: () => void;
  onOpenFolder: () => Promise<void>;
  onExportDocx: () => Promise<void>;
  onExportPdf: () => Promise<void>;
  hasSummary: boolean;
}

export function SummaryUpdaterButtonGroup({
  isSaving,
  isDirty,
  onSave,
  onOpenFolder,
  onExportDocx,
  onExportPdf,
  hasSummary,
}: SummaryUpdaterButtonGroupProps) {
  return (
    <ButtonGroup>
      {/* Save */}
      <Button
        variant="outline"
        size="sm"
        className={isDirty
          ? 'bg-[#16478e] text-white border-[#16478e] hover:bg-[#1a55ab] hover:border-[#1a55ab]'
          : ''}
        title={isSaving ? 'Đang lưu' : isDirty ? 'Có thay đổi chưa lưu' : 'Lưu thay đổi'}
        onClick={() => {
          Analytics.trackButtonClick('save_changes', 'meeting_details');
          onSave();
        }}
        disabled={isSaving}
      >
        {isSaving ? (
          <>
            <Loader2 className="animate-spin" />
            <span>Đang lưu...</span>
          </>
        ) : (
          <>
            <Save />
            <span>Lưu</span>
          </>
        )}
      </Button>

      {/* Export dropdown: DOCX + PDF */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="outline"
            size="sm"
            title="Xuất file"
            disabled={!hasSummary}
            className="cursor-pointer gap-1"
          >
            <Download className="h-4 w-4" />
            <span>Xuất</span>
            <ChevronDown className="h-3 w-3 opacity-60" />
          </Button>
        </DropdownMenuTrigger>

        <DropdownMenuContent align="end" className="min-w-[140px]">
          <DropdownMenuItem
            onClick={() => {
              Analytics.trackButtonClick('export_summary_docx', 'meeting_details');
              onExportDocx();
            }}
            className="gap-2 cursor-pointer"
          >
            <FileDown className="h-4 w-4 text-blue-600" />
            <span>Xuất DOCX</span>
          </DropdownMenuItem>

          <DropdownMenuItem
            onClick={() => {
              Analytics.trackButtonClick('export_summary_pdf', 'meeting_details');
              onExportPdf();
            }}
            className="gap-2 cursor-pointer"
          >
            <FileText className="h-4 w-4 text-red-500" />
            <span>Xuất PDF</span>
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </ButtonGroup>
  );
}
