'use client';

import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import type { EditorMode, TemplateData, TemplateInfo, TemplateSection } from './types';

const EMPTY_SECTION: TemplateSection = {
  title: '',
  instruction: '',
  format: 'paragraph',
};

function slugify(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9\s_-]/g, '')
    .trim()
    .replace(/\s+/g, '_')
    .slice(0, 40);
}

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
  const [editingId, setEditingId] = useState('');
  const [isSaving, setIsSaving] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);

  useEffect(() => {
    invoke<string>('api_get_default_template')
      .then(id => setDefaultTemplateId(id))
      .catch(() => {});
  }, []);

  const loadTemplates = useCallback(async () => {
    setIsLoadingList(true);
    try {
      const list = await invoke<TemplateInfo[]>('api_list_templates');
      setTemplates(list);
    } catch (err) {
      toast.error(`Không tải được danh sách mẫu: ${err}`);
    } finally {
      setIsLoadingList(false);
    }
  }, []);

  const openTemplate = useCallback(async (id: string) => {
    try {
      const jsonStr = await invoke<string>('api_get_template_json', { templateId: id });
      const parsed: TemplateData = JSON.parse(jsonStr);
      setEditorData(withKeys(parsed));
      setSelectedId(id);
      setEditingId(id);
      setEditorMode('edit');
    } catch (err) {
      toast.error(`Không mở được mẫu: ${err}`);
    }
  }, []);

  const cloneTemplate = useCallback(async (id: string) => {
    try {
      const jsonStr = await invoke<string>('api_get_template_json', { templateId: id });
      const parsed: TemplateData = JSON.parse(jsonStr);
      parsed.name = `${parsed.name} (bản sao)`;
      const newId = `copy_of_${slugify(id)}`;
      setEditorData(withKeys(parsed));
      setEditingId(newId);
      setSelectedId(null);
      setEditorMode('new');
    } catch (err) {
      toast.error(`Không sao chép được mẫu: ${err}`);
    }
  }, []);

  const startNewTemplate = useCallback(() => {
    setEditorData({ name: '', description: '', sections: [{ ...EMPTY_SECTION, _key: crypto.randomUUID() }] });
    setEditingId('');
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
    if (!editorData || !editingId) {
      toast.error('Thiếu thông tin mẫu hoặc ID');
      return;
    }

    const templateJson = JSON.stringify(stripKeys(editorData), null, 2);

    setIsSaving(true);
    try {
      await invoke('api_save_custom_template', {
        templateId: editingId,
        templateJson,
      });
      toast.success('Đã lưu mẫu thành công');
      await loadTemplates();
      // Re-open to sync state
      await openTemplate(editingId);
    } catch (err) {
      toast.error(`Lưu mẫu thất bại: ${err}`);
    } finally {
      setIsSaving(false);
    }
  }, [editorData, editingId, loadTemplates, openTemplate]);

  const deleteTemplate = useCallback(async (id: string) => {
    setIsDeleting(true);
    try {
      await invoke('api_delete_custom_template', { templateId: id });
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
      await invoke('api_set_default_template', { templateId: id });
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
    editingId,
    setEditingId,
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
    removeSection,
    moveSection,
    updateSection,
  };
}
