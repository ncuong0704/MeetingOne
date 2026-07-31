import { useState, useEffect, useCallback } from 'react';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';
import { getDefaultTemplate, listTemplates } from '@/services/templateService';

export function useTemplates() {
  const [availableTemplates, setAvailableTemplates] = useState<Array<{
    id: string;
    name: string;
    description: string;
  }>>([]);
  const [selectedTemplate, setSelectedTemplate] = useState<string>('theo_mau_act_no_table');

  // Fetch available templates and saved default on mount
  useEffect(() => {
    const init = async () => {
      try {
        const [templates, defaultId] = await Promise.all([
          listTemplates(),
          getDefaultTemplate(),
        ]);
        setAvailableTemplates(templates);
        setSelectedTemplate(defaultId);
      } catch (error) {
        console.error('Failed to fetch templates:', error);
      }
    };
    init();
  }, []);

  // Handle template selection
  const handleTemplateSelection = useCallback((templateId: string, templateName: string) => {
    setSelectedTemplate(templateId);
    toast.success('Đã chọn mẫu', {
      description: `Đang dùng mẫu «${templateName}» để tạo tóm tắt`,
    });
    Analytics.trackFeatureUsed('template_selected');
  }, []);

  return {
    availableTemplates,
    selectedTemplate,
    handleTemplateSelection,
  };
}
