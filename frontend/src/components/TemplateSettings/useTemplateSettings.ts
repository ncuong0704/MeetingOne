'use client';

import { useState, useCallback, useEffect } from 'react';
import { toast } from 'sonner';
import type { EditorMode, TemplateData, TemplateInfo, TemplateSection } from './types';
import {
  deleteCustomTemplate,
  getDefaultTemplate,
  getTemplateJson,
  listTemplates,
  saveCustomTemplate,
  setDefaultTemplate,
} from '@/services/templateService';
import { generateUniqueTemplateId } from './templateIdUtils';

const EMPTY_SECTION: TemplateSection = {
  title: '',
  instruction: '',
  format: 'paragraph',
};

function withKeys(data: TemplateData): TemplateData {
  return {
    ...data,
    sections: data.sections.map(s => ({ ...s, _key: crypto.randomUUID() })),
  };
}

function stripKeys(data: TemplateData): TemplateData {
  return {
    ...data,
    sections: data.sections.map(({ _key, ...rest }) => rest),
  };
}

export function useTemplateSettings() {
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [isLoadingList, setIsLoadingList] = useState(false);
  const [defaultTemplateId, setDefaultTemplateId] = useState<string>('theo_mau_act_no_table');
  const [isSettingDefault, setIsSettingDefault] = useState(false);

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [editorMode, setEditorMode] = useState<EditorMode>('idle');
  const [editorData, setEditorData] = useState<TemplateData | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);

  useEffect(() => {
    getDefaultTemplate()
      .then(id => setDefaultTemplateId(id))
      .catch(() => {});
  }, []);

  const loadTemplates = useCallback(async () => {
    setIsLoadingList(true);
    try {
      const list = await listTemplates();
      setTemplates(list);
    } catch (err) {
      toast.error(`Không tải được danh sách mẫu: ${err}`);
    } finally {
      setIsLoadingList(false);
    }
  }, []);

  const openTemplate = useCallback(async (id: string) => {
    try {
      const jsonStr = await getTemplateJson(id);
      const parsed: TemplateData = JSON.parse(jsonStr);
      setEditorData(withKeys(parsed));
      setSelectedId(id);
      setEditorMode('edit');
    } catch (err) {
      toast.error(`Không mở được mẫu: ${err}`);
    }
  }, []);

  const cloneTemplate = useCallback(async (id: string) => {
    try {
      const jsonStr = await getTemplateJson(id);
      const parsed: TemplateData = JSON.parse(jsonStr);
      parsed.name = `${parsed.name} (bản sao)`;
      setEditorData(withKeys(parsed));
      setSelectedId(null);
      setEditorMode('new');
    } catch (err) {
      toast.error(`Không sao chép được mẫu: ${err}`);
    }
  }, []);

  const startNewTemplate = useCallback(() => {
    setEditorData({ name: '', description: '', sections: [{ ...EMPTY_SECTION, _key: crypto.randomUUID() }] });
    setSelectedId(null);
    setEditorMode('new');
  }, []);

  const cancelEdit = useCallback(async () => {
    if (selectedId && editorMode === 'edit') {
      // Reload original data
      await openTemplate(selectedId);
    } else {
      setEditorData(null);
      setSelectedId(null);
      setEditorMode('idle');
    }
  }, [selectedId, editorMode, openTemplate]);

  const closeEditor = useCallback(() => {
    setEditorData(null);
    setSelectedId(null);
    setEditorMode('idle');
  }, []);

  const saveTemplate = useCallback(async () => {
    if (!editorData) {
      toast.error('Thiếu thông tin mẫu');
      return;
    }

    let templateId: string;
    if (editorMode === 'edit' && selectedId) {
      templateId = selectedId;
    } else {
      templateId = generateUniqueTemplateId(
        editorData.name,
        templates.map((t) => t.id),
      );
      if (!templateId) {
        toast.error('Tên mẫu không hợp lệ — cần ít nhất một chữ cái hoặc số');
        return;
      }
    }

    const templateJson = JSON.stringify(stripKeys(editorData), null, 2);

    setIsSaving(true);
    try {
      await saveCustomTemplate(templateId, templateJson);
      toast.success('Đã lưu mẫu thành công');
      await loadTemplates();
      await openTemplate(templateId);
    } catch (err) {
      toast.error(`Lưu mẫu thất bại: ${err}`);
    } finally {
      setIsSaving(false);
    }
  }, [editorData, editorMode, selectedId, templates, loadTemplates, openTemplate]);

  const deleteTemplate = useCallback(async (id: string) => {
    setIsDeleting(true);
    try {
      await deleteCustomTemplate(id);
      toast.success('Đã xóa mẫu tùy chỉnh');
      setEditorData(null);
      setSelectedId(null);
      setEditorMode('idle');
      await loadTemplates();
    } catch (err) {
      toast.error(`Xóa mẫu thất bại: ${err}`);
    } finally {
      setIsDeleting(false);
    }
  }, [loadTemplates]);

  // --- Section mutations ---

  const updateMeta = useCallback((field: 'name' | 'description', value: string) => {
    setEditorData(prev => prev ? { ...prev, [field]: value } : prev);
  }, []);

  const addSection = useCallback(() => {
    setEditorData(prev => {
      if (!prev) return prev;
      return { ...prev, sections: [...prev.sections, { ...EMPTY_SECTION, _key: crypto.randomUUID() }] };
    });
  }, []);

  const ensureMinSections = useCallback((min: number) => {
    setEditorData(prev => {
      if (!prev) return prev;
      const sections = [...prev.sections];
      while (sections.length < min) {
        sections.push({ ...EMPTY_SECTION, _key: crypto.randomUUID() });
      }
      return { ...prev, sections };
    });
  }, []);

  const removeSection = useCallback((index: number) => {
    setEditorData(prev => {
      if (!prev) return prev;
      const sections = [...prev.sections];
      sections.splice(index, 1);
      return { ...prev, sections };
    });
  }, []);

  const moveSection = useCallback((index: number, direction: 'up' | 'down') => {
    setEditorData(prev => {
      if (!prev) return prev;
      const sections = [...prev.sections];
      const target = direction === 'up' ? index - 1 : index + 1;
      if (target < 0 || target >= sections.length) return prev;
      [sections[index], sections[target]] = [sections[target], sections[index]];
      return { ...prev, sections };
    });
  }, []);

  const updateSection = useCallback((index: number, field: keyof TemplateSection, value: string) => {
    setEditorData(prev => {
      if (!prev) return prev;
      const sections = [...prev.sections];
      sections[index] = { ...sections[index], [field]: value };
      return { ...prev, sections };
    });
  }, []);

  const setAsDefault = useCallback(async (id: string, name: string) => {
    setIsSettingDefault(true);
    try {
      await setDefaultTemplate(id);
      setDefaultTemplateId(id);
      toast.success(`Đã đặt «${name}» làm mẫu mặc định`);
    } catch (err) {
      toast.error(`Không đặt được mẫu mặc định: ${err}`);
    } finally {
      setIsSettingDefault(false);
    }
  }, []);

  return {
    templates,
    isLoadingList,
    loadTemplates,
    defaultTemplateId,
    isSettingDefault,
    setAsDefault,
    selectedId,
    editorMode,
    editorData,
    isSaving,
    isDeleting,
    openTemplate,
    cloneTemplate,
    startNewTemplate,
    cancelEdit,
    closeEditor,
    saveTemplate,
    deleteTemplate,
    updateMeta,
    addSection,
    ensureMinSections,
    removeSection,
    moveSection,
    updateSection,
  };
}
