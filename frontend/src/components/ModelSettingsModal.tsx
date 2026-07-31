import { useState, useEffect, useRef, forwardRef, useImperativeHandle, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useConfig } from '@/contexts/ConfigContext';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Lock, Unlock, Eye, EyeOff, RefreshCw, CheckCircle2, ChevronDown, ChevronUp, Check, ChevronsUpDown, Search } from 'lucide-react';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/components/ui/command';
import { cn } from '@/lib/utils';
import { toast } from 'sonner';
import {
  DEFAULT_CUSTOM_OPENAI_ENDPOINT,
  DEFAULT_CUSTOM_OPENAI_MODEL,
} from '@/constants/modelDefaults';

export interface ModelConfig {
  provider: 'claude' | 'openai' | 'openrouter' | 'custom-openai';
  model: string;
  apiKey?: string | null;
  // Custom OpenAI fields
  customOpenAIEndpoint?: string | null;
  customOpenAIModel?: string | null;
  customOpenAIApiKey?: string | null;
  maxTokens?: number | null;
  temperature?: number | null;
  topP?: number | null;
  /** Per-provider fallback model list. Key = provider name, value = ordered list of fallback models. */
  fallbackModels?: Record<string, string[]> | null;
}

interface OpenRouterModel {
  id: string;
  name: string;
  context_length?: number;
  prompt_price?: string;
  completion_price?: string;
}

interface OpenAIModel {
  id: string;
}

interface AnthropicModel {
  id: string;
  display_name?: string;
}

// Fallback models for when API fetch fails or no API key provided
const OPENAI_FALLBACK_MODELS = [
  'gpt-4o',
  'gpt-4o-mini',
  'gpt-4-turbo',
  'gpt-4',
  'gpt-3.5-turbo',
  'o1',
  'o1-mini',
  'o3',
  'o3-mini',
];

const CLAUDE_FALLBACK_MODELS = [
  'claude-sonnet-4-5-20250929',
  'claude-haiku-4-5-20251001',
  'claude-opus-4-5-20251101',
  'claude-3-5-sonnet-latest',
];

export interface ModelSettingsModalRef {
  save: () => Promise<boolean>;
  getConfig: () => ModelConfig;
}

interface ModelSettingsModalProps {
  modelConfig: ModelConfig;
  setModelConfig: (config: ModelConfig | ((prev: ModelConfig) => ModelConfig)) => void;
  onSave: (config: ModelConfig) => void | Promise<void>;
  skipInitialFetch?: boolean;
  embedded?: boolean;
  allowSkipApiKey?: boolean;
}

export const ModelSettingsModal = forwardRef<ModelSettingsModalRef, ModelSettingsModalProps>(function ModelSettingsModal({
  modelConfig: propsModelConfig,
  setModelConfig: propsSetModelConfig,
  onSave,
  skipInitialFetch = false,
  embedded = false,
  allowSkipApiKey = false,
}, ref) {
  const configContext = useConfig();
  // Parent-managed config (embedded or skipInitialFetch): always use props, not ConfigContext
  const modelConfig = (embedded || skipInitialFetch)
    ? propsModelConfig
    : (configContext?.modelConfig || propsModelConfig);
  const setModelConfig = (embedded || skipInitialFetch)
    ? propsSetModelConfig
    : (configContext?.setModelConfig || propsSetModelConfig);
  const providerApiKeys = configContext?.providerApiKeys;
  const updateProviderApiKey = configContext?.updateProviderApiKey;

  const [apiKey, setApiKey] = useState<string | null>(modelConfig.apiKey || null);
  const [showApiKey, setShowApiKey] = useState<boolean>(false);
  const [isApiKeyLocked, setIsApiKeyLocked] = useState<boolean>(!!modelConfig.apiKey?.trim());
  const [isLockButtonVibrating, setIsLockButtonVibrating] = useState<boolean>(false);
  const [openRouterModels, setOpenRouterModels] = useState<OpenRouterModel[]>([]);
  const [openRouterError, setOpenRouterError] = useState<string>('');
  const [isLoadingOpenRouter, setIsLoadingOpenRouter] = useState<boolean>(false);
  const hasLoadedInitialConfig = useRef<boolean>(false);
  const [fallbackSearchQuery, setFallbackSearchQuery] = useState<string>('');

  // Custom OpenAI state
  const [customOpenAIEndpoint, setCustomOpenAIEndpoint] = useState<string>(
    modelConfig.customOpenAIEndpoint || DEFAULT_CUSTOM_OPENAI_ENDPOINT
  );
  const [customOpenAIModel, setCustomOpenAIModel] = useState<string>(
    modelConfig.customOpenAIModel || DEFAULT_CUSTOM_OPENAI_MODEL
  );
  const [customOpenAIApiKey, setCustomOpenAIApiKey] = useState<string>(modelConfig.customOpenAIApiKey || '');
  const [customMaxTokens, setCustomMaxTokens] = useState<string>(modelConfig.maxTokens?.toString() || '');
  const [customTemperature, setCustomTemperature] = useState<string>(modelConfig.temperature?.toString() || '');
  const [customTopP, setCustomTopP] = useState<string>(modelConfig.topP?.toString() || '');
  const [isCustomOpenAIAdvancedOpen, setIsCustomOpenAIAdvancedOpen] = useState<boolean>(false);
  const [isTestingConnection, setIsTestingConnection] = useState<boolean>(false);

  // Combobox state
  const [modelComboboxOpen, setModelComboboxOpen] = useState<boolean>(false);

  // Fallback models state: ordered list for the current provider
  const [fallbackModels, setFallbackModels] = useState<string[]>([]);

  // When skipInitialFetch=true the component never calls api_get_model_config itself,
  // so we must sync fallbackModels from propsModelConfig whenever it changes.
  useEffect(() => {
    if (!skipInitialFetch) return;
    const raw = propsModelConfig.fallbackModels;
    const map: Record<string, string[]> =
      !raw ? {} :
      typeof raw === 'string'
        ? (() => { try { return JSON.parse(raw); } catch { return {}; } })()
        : (raw as Record<string, string[]>);
    setFallbackModels(map[propsModelConfig.provider] ?? []);
    setFallbackSearchQuery('');
  }, [skipInitialFetch, propsModelConfig.fallbackModels, propsModelConfig.provider]);

  // Dynamic model fetching state for OpenAI and Claude
  const [openaiModels, setOpenaiModels] = useState<string[]>([]);
  const [claudeModels, setClaudeModels] = useState<string[]>([]);
  const [isLoadingOpenAI, setIsLoadingOpenAI] = useState<boolean>(false);
  const [isLoadingClaude, setIsLoadingClaude] = useState<boolean>(false);

  const fetchApiKey = async (provider: string) => {
    try {
      const data = (await invoke('api_get_api_key', {
        provider,
      })) as string;
      const trimmed = data?.trim() || '';
      setApiKey(trimmed || null);
      setIsApiKeyLocked(!!trimmed);
    } catch (err) {
      console.error('Error fetching API key:', err);
      setApiKey(null);
      setIsApiKeyLocked(false);
    }
  };

  /** Load the API key for a specific provider — never leak another provider's key into the input. */
  const applyApiKeyForProvider = useCallback((provider: ModelConfig['provider']) => {
    if (provider === 'custom-openai') return;
    const fromContext = providerApiKeys?.[provider as keyof typeof providerApiKeys]?.trim();
    if (fromContext) {
      setApiKey(fromContext);
      setIsApiKeyLocked(true);
      return;
    }
    setApiKey(null);
    setIsApiKeyLocked(false);
    void fetchApiKey(provider);
  }, [providerApiKeys]);

  // Auto-unlock when API key becomes empty, 
  useEffect(() => {
    const hasContent = !!apiKey?.trim();
    if (!hasContent) {
      setIsApiKeyLocked(false);
    }
  }, [apiKey]);

  const modelOptions: Record<string, string[]> = {
    claude: claudeModels.length > 0 ? claudeModels : CLAUDE_FALLBACK_MODELS,
    openai: openaiModels.length > 0 ? openaiModels : OPENAI_FALLBACK_MODELS,
    openrouter: openRouterModels.map((m) => m.id),
    'custom-openai': customOpenAIModel ? [customOpenAIModel] : [], // User specifies model manually
  };

  const requiresApiKey =
    modelConfig.provider === 'claude' ||
    modelConfig.provider === 'openai' ||
    modelConfig.provider === 'openrouter';

  // Custom OpenAI validation — only invalid when user partially filled fields
  const isCustomOpenAIPartiallyFilled =
    customOpenAIEndpoint.trim() !== '' || customOpenAIModel.trim() !== '';
  const isCustomOpenAIInvalid = modelConfig.provider === 'custom-openai' && (
    allowSkipApiKey
      ? isCustomOpenAIPartiallyFilled && (!customOpenAIEndpoint.trim() || !customOpenAIModel.trim())
      : !customOpenAIEndpoint.trim() || !customOpenAIModel.trim()
  );

  const isDoneDisabled =
    (!allowSkipApiKey && requiresApiKey && (!apiKey || (typeof apiKey === 'string' && !apiKey.trim()))) ||
    isCustomOpenAIInvalid;

  useEffect(() => {
    const fetchModelConfig = async () => {
      // If parent component manages config, skip fetch and just mark as loaded
      if (skipInitialFetch) {
        hasLoadedInitialConfig.current = true;
        return;
      }

      try {
        const data = (await invoke('api_get_model_config')) as any;
        if (data && data.provider !== null) {
          setModelConfig(data);

          // Fetch API key if not included in response and provider requires it
          if (!data.apiKey) {
            try {
              const apiKeyData = await invoke('api_get_api_key', {
                provider: data.provider
              }) as string;
              data.apiKey = apiKeyData;
              setApiKey(apiKeyData);
            } catch (err) {
              console.error('Failed to fetch API key:', err);
            }
          }

          // Parse fallbackModels JSON map and extract list for current provider
          if (data.fallbackModels) {
            try {
              const map: Record<string, string[]> =
                typeof data.fallbackModels === 'string'
                  ? JSON.parse(data.fallbackModels)
                  : data.fallbackModels;
              setFallbackModels(map[data.provider] ?? []);
            } catch {
              setFallbackModels([]);
            }
          }

          hasLoadedInitialConfig.current = true; // Mark that initial config is loaded

          // Fetch Custom OpenAI config if that's the active provider
          if (data.provider === 'custom-openai') {
            try {
              const customConfig = (await invoke('api_get_custom_openai_config')) as any;
              if (customConfig) {
                setCustomOpenAIEndpoint(customConfig.endpoint || '');
                setCustomOpenAIModel(customConfig.model || '');
                setCustomOpenAIApiKey(customConfig.apiKey || '');
                setCustomMaxTokens(customConfig.maxTokens?.toString() || '');
                setCustomTemperature(customConfig.temperature?.toString() || '');
                setCustomTopP(customConfig.topP?.toString() || '');
              }
            } catch (err) {
              console.error('Failed to fetch custom OpenAI config:', err);
            }
          }
        }
      } catch (error) {
        console.error('Failed to fetch model config:', error);
        hasLoadedInitialConfig.current = true; // Mark as loaded even on error
      }
    };

    fetchModelConfig();
  }, [skipInitialFetch]);

  // Sync custom OpenAI state from modelConfig (context or props)
  useEffect(() => {
    if (modelConfig.provider === 'custom-openai') {
      console.log('Syncing custom OpenAI fields from ConfigContext:', {
        endpoint: modelConfig.customOpenAIEndpoint,
        model: modelConfig.customOpenAIModel,
        hasApiKey: !!modelConfig.customOpenAIApiKey,
      });

      // Always sync from modelConfig (which comes from context if available)
      setCustomOpenAIEndpoint(modelConfig.customOpenAIEndpoint || DEFAULT_CUSTOM_OPENAI_ENDPOINT);
      setCustomOpenAIModel(modelConfig.customOpenAIModel || DEFAULT_CUSTOM_OPENAI_MODEL);
      setCustomOpenAIApiKey(modelConfig.customOpenAIApiKey || '');
      setCustomMaxTokens(modelConfig.maxTokens?.toString() || '');
      setCustomTemperature(modelConfig.temperature?.toString() || '');
      setCustomTopP(modelConfig.topP?.toString() || '');
    }
  }, [
    modelConfig.provider,
    modelConfig.customOpenAIEndpoint,
    modelConfig.customOpenAIModel,
    modelConfig.customOpenAIApiKey,
    modelConfig.maxTokens,
    modelConfig.temperature,
    modelConfig.topP
  ]);

  // Sync local apiKey to the active provider's key (clear stale key when switching providers)
  useEffect(() => {
    if (!requiresApiKey || modelConfig.provider === 'custom-openai') return;
    const provider = modelConfig.provider;
    const correctKey = providerApiKeys?.[provider as keyof typeof providerApiKeys]?.trim() ?? '';
    if (providerApiKeys) {
      const parentKey = modelConfig.apiKey?.trim() ?? '';
      const effectiveKey = parentKey || correctKey;
      setApiKey(effectiveKey || null);
      setIsApiKeyLocked(!!effectiveKey);
    } else if (skipInitialFetch) {
      const parentKey = modelConfig.apiKey?.trim();
      if (parentKey && modelConfig.provider === provider) {
        setApiKey(parentKey);
        setIsApiKeyLocked(true);
      }
    } else {
      void fetchApiKey(provider);
    }
  }, [modelConfig.provider, modelConfig.apiKey, providerApiKeys, requiresApiKey, skipInitialFetch]);

  // When parent config updates from another surface (event sync), refresh local API key state
  useEffect(() => {
    if (!skipInitialFetch) return;
    if (!requiresApiKey || modelConfig.provider === 'custom-openai') return;
    const parentKey = propsModelConfig.apiKey?.trim();
    if (parentKey && propsModelConfig.provider === modelConfig.provider) {
      setApiKey(parentKey);
      setIsApiKeyLocked(true);
    }
  }, [
    propsModelConfig.apiKey,
    propsModelConfig.provider,
    modelConfig.provider,
    requiresApiKey,
    skipInitialFetch,
  ]);

  const loadOpenRouterModels = async () => {
    if (openRouterModels.length > 0) return; // Already loaded

    try {
      setIsLoadingOpenRouter(true);
      setOpenRouterError('');
      const data = (await invoke('get_openrouter_models')) as OpenRouterModel[];
      setOpenRouterModels(data);
    } catch (err) {
      console.error('Error loading OpenRouter models:', err);
      setOpenRouterError(
        err instanceof Error ? err.message : 'Không tải được mô hình OpenRouter'
      );
    } finally {
      setIsLoadingOpenRouter(false);
    }
  };

  // Fetch OpenAI models from API
  const loadOpenAIModels = async (key: string | null) => {
    if (!key?.trim()) {
      setOpenaiModels([]); // Will use fallback via modelOptions
      return;
    }
    setIsLoadingOpenAI(true);
    try {
      const data = (await invoke('get_openai_models', { apiKey: key })) as OpenAIModel[];
      setOpenaiModels(data.map((m) => m.id));
    } catch (err) {
      console.error('Error loading OpenAI models:', err);
      setOpenaiModels([]); // Will use fallback via modelOptions
    } finally {
      setIsLoadingOpenAI(false);
    }
  };

  // Fetch Anthropic (Claude) models from API
  const loadClaudeModels = async (key: string | null) => {
    if (!key?.trim()) {
      setClaudeModels([]); // Will use fallback via modelOptions
      return;
    }
    setIsLoadingClaude(true);
    try {
      const data = (await invoke('get_anthropic_models', { apiKey: key })) as AnthropicModel[];
      setClaudeModels(data.map((m) => m.id));
    } catch (err) {
      console.error('Error loading Claude models:', err);
      setClaudeModels([]); // Will use fallback via modelOptions
    } finally {
      setIsLoadingClaude(false);
    }
  };

  // Auto-fetch OpenAI models when provider is openai and we have an API key
  useEffect(() => {
    if (modelConfig.provider === 'openai' && apiKey?.trim()) {
      loadOpenAIModels(apiKey);
    }
  }, [modelConfig.provider, apiKey]);

  // Auto-fetch Claude models when provider is claude and we have an API key
  useEffect(() => {
    if (modelConfig.provider === 'claude' && apiKey?.trim()) {
      loadClaudeModels(apiKey);
    }
  }, [modelConfig.provider, apiKey]);

  // Auto-fetch OpenRouter models when provider is already openrouter (e.g. saved config on mount)
  useEffect(() => {
    if (modelConfig.provider === 'openrouter') {
      void loadOpenRouterModels();
    }
  }, [modelConfig.provider]);

  // Restore cached model when async model lists become available
  useEffect(() => {
    const providerModels = modelOptions[modelConfig.provider];
    if (!providerModels || providerModels.length === 0) return;

    // If current model is already valid, nothing to do
    if (modelConfig.model && providerModels.includes(modelConfig.model)) return;

    // Try to restore from localStorage cache
    const map = JSON.parse(localStorage.getItem('providerModelMap') || '{}');
    const cachedModel = map[modelConfig.provider];
    if (cachedModel && providerModels.includes(cachedModel)) {
      setModelConfig((prev: ModelConfig) => ({ ...prev, model: cachedModel }));
    }
  }, [openRouterModels, openaiModels, claudeModels, modelConfig.provider]);

  const buildUpdatedConfig = useCallback((): ModelConfig => ({
    ...modelConfig,
    apiKey: typeof apiKey === 'string' ? apiKey.trim() || null : null,
    customOpenAIEndpoint: modelConfig.provider === 'custom-openai' ? customOpenAIEndpoint.trim() : null,
    customOpenAIModel: modelConfig.provider === 'custom-openai' ? customOpenAIModel.trim() : null,
    customOpenAIApiKey: modelConfig.provider === 'custom-openai' && customOpenAIApiKey.trim() ? customOpenAIApiKey.trim() : null,
    maxTokens: modelConfig.provider === 'custom-openai' && customMaxTokens ? parseInt(customMaxTokens, 10) : null,
    temperature: modelConfig.provider === 'custom-openai' && customTemperature ? parseFloat(customTemperature) : null,
    topP: modelConfig.provider === 'custom-openai' && customTopP ? parseFloat(customTopP) : null,
    model: modelConfig.provider === 'custom-openai' ? customOpenAIModel.trim() : modelConfig.model,
    fallbackModels: (() => {
      // modelConfig.fallbackModels may be a raw JSON string (from Rust/backend) or a parsed object.
      // Always parse to object before spreading to avoid corrupting the map.
      const raw = modelConfig.fallbackModels;
      let existingMap: Record<string, string[]> = {};
      if (raw) {
        if (typeof raw === 'string') {
          try { existingMap = JSON.parse(raw); } catch { existingMap = {}; }
        } else {
          existingMap = raw as Record<string, string[]>;
        }
      }
      return { ...existingMap, [modelConfig.provider]: fallbackModels };
    })(),
  }), [
    modelConfig, apiKey, customOpenAIEndpoint, customOpenAIModel,
    customOpenAIApiKey, customMaxTokens, customTemperature, customTopP, fallbackModels,
  ]);

  const persistConfig = useCallback(async (updatedConfig: ModelConfig) => {
    setModelConfig(updatedConfig);
    if (updatedConfig.model) {
      const map = JSON.parse(localStorage.getItem('providerModelMap') || '{}');
      map[updatedConfig.provider] = updatedConfig.model;
      localStorage.setItem('providerModelMap', JSON.stringify(map));
    }
    if (updateProviderApiKey && updatedConfig.apiKey && updatedConfig.provider !== 'custom-openai') {
      updateProviderApiKey(updatedConfig.provider, updatedConfig.apiKey);
    }
    await Promise.resolve(onSave(updatedConfig));
  }, [setModelConfig, updateProviderApiKey, onSave]);

  const save = useCallback(async (): Promise<boolean> => {
    if (isDoneDisabled) return false;
    if (modelConfig.provider === 'custom-openai') {
      const hasFullCustomConfig = customOpenAIEndpoint.trim() && customOpenAIModel.trim();
      if (hasFullCustomConfig) {
        try {
          await invoke('api_save_custom_openai_config', {
            endpoint: customOpenAIEndpoint.trim(),
            apiKey: customOpenAIApiKey.trim() || null,
            model: customOpenAIModel.trim(),
            maxTokens: customMaxTokens ? parseInt(customMaxTokens, 10) : null,
            temperature: customTemperature ? parseFloat(customTemperature) : null,
            topP: customTopP ? parseFloat(customTopP) : null,
          });
        } catch (err) {
          console.error('Failed to save custom OpenAI config:', err);
          if (!allowSkipApiKey) {
            toast.error('Không lưu được cấu hình OpenAI tùy chỉnh');
          }
          return false;
        }
      } else if (!allowSkipApiKey) {
        return false;
      }
    }

    const updatedConfig = buildUpdatedConfig();
    await persistConfig(updatedConfig);
    return true;
  }, [
    isDoneDisabled, modelConfig.provider, customOpenAIEndpoint, customOpenAIModel, customOpenAIApiKey,
    customMaxTokens, customTemperature, customTopP, allowSkipApiKey,
    buildUpdatedConfig, persistConfig,
  ]);

  useImperativeHandle(ref, () => ({
    save,
    getConfig: buildUpdatedConfig,
  }), [save, buildUpdatedConfig]);

  const handleSave = async () => {
    if (isDoneDisabled) return;
    const ok = await save();
    if (!ok && !allowSkipApiKey) {
      toast.error('Không lưu được cài đặt mô hình');
    }
  };

  // Test custom OpenAI connection
  const testCustomOpenAIConnection = async () => {
    if (!customOpenAIEndpoint.trim() || !customOpenAIModel.trim()) {
      toast.error('Vui lòng nhập URL endpoint và tên mô hình trước');
      return;
    }

    setIsTestingConnection(true);
    try {
      const result = await invoke<{ status: string; message: string }>('api_test_custom_openai_connection', {
        endpoint: customOpenAIEndpoint.trim(),
        apiKey: customOpenAIApiKey.trim() || null,
        model: customOpenAIModel.trim(),
      });
      toast.success(result.message || 'Kết nối thành công!');
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      toast.error(errorMsg);
    } finally {
      setIsTestingConnection(false);
    }
  };

  const handleInputClick = () => {
    if (isApiKeyLocked) {
      setIsLockButtonVibrating(true);
      setTimeout(() => setIsLockButtonVibrating(false), 500);
    }
  };

  const providerModelsCount = modelOptions[modelConfig.provider]?.length ?? 0;
  const hasModelsForProvider = providerModelsCount > 0;
  const selectedModelLabel = hasModelsForProvider
    ? (modelConfig.model || 'Chọn mô hình…')
    : 'Chưa cài đặt';

  return (
    <div>
      {!embedded && (
        <div className="flex justify-between items-center mb-4">
          <h3 className="text-lg font-semibold">Cài đặt mô hình</h3>
        </div>
      )}

      <div className="space-y-4">
        <div>
          <Label>Mô hình tóm tắt</Label>
          <div className="flex space-x-2 mt-1">
            <Select
              value={modelConfig.provider}
              onValueChange={(value) => {
                const provider = value as ModelConfig['provider'];

                // Save current provider's model to localStorage before switching
                const map = JSON.parse(localStorage.getItem('providerModelMap') || '{}');
                if (modelConfig.model) {
                  map[modelConfig.provider] = modelConfig.model;
                  localStorage.setItem('providerModelMap', JSON.stringify(map));
                }

                // Try to restore cached model for the new provider
                const savedModel = map[provider];
                const providerModels = modelOptions[provider];
                const defaultModel = providerModels && providerModels.length > 0
                  ? providerModels[0]
                  : '';
                const model = (savedModel && providerModels?.includes(savedModel))
                  ? savedModel
                  : defaultModel;

                const keyForProvider =
                  providerApiKeys?.[provider as keyof typeof providerApiKeys]?.trim() || null;
                applyApiKeyForProvider(provider);
                setModelConfig({
                  ...modelConfig,
                  provider,
                  model,
                  apiKey: keyForProvider,
                });
                // Reset fallback models to the saved list for the new provider.
                // modelConfig.fallbackModels may be a raw JSON string — parse safely.
                const rawMap = modelConfig.fallbackModels;
                const savedFallbackMap: Record<string, string[]> =
                  !rawMap ? {} :
                  typeof rawMap === 'string' ? (() => { try { return JSON.parse(rawMap); } catch { return {}; } })() :
                  rawMap as Record<string, string[]>;
                setFallbackModels(savedFallbackMap[provider] ?? []);
                // API key is now synced automatically via useEffect watching providerApiKeys

                // Load OpenRouter models only when OpenRouter is selected
                if (provider === 'openrouter') {
                  loadOpenRouterModels();
                }

                // Load custom OpenAI config when selected
                if (provider === 'custom-openai') {
                  invoke<any>('api_get_custom_openai_config').then((config) => {
                    const endpoint = config?.endpoint || DEFAULT_CUSTOM_OPENAI_ENDPOINT;
                    const model = config?.model || DEFAULT_CUSTOM_OPENAI_MODEL;
                    setCustomOpenAIEndpoint(endpoint);
                    setCustomOpenAIModel(model);
                    setCustomOpenAIApiKey(config?.apiKey || '');
                    setCustomMaxTokens(config?.maxTokens?.toString() || '');
                    setCustomTemperature(config?.temperature?.toString() || '');
                    setCustomTopP(config?.topP?.toString() || '');
                    setModelConfig((prev: ModelConfig) => ({
                      ...prev,
                      provider,
                      model,
                      customOpenAIEndpoint: endpoint,
                      customOpenAIModel: model,
                      customOpenAIApiKey: config?.apiKey || null,
                      maxTokens: config?.maxTokens ?? null,
                      temperature: config?.temperature ?? null,
                      topP: config?.topP ?? null,
                    }));
                  }).catch((err) => {
                    console.error('Failed to load custom OpenAI config:', err);
                    setCustomOpenAIEndpoint(DEFAULT_CUSTOM_OPENAI_ENDPOINT);
                    setCustomOpenAIModel(DEFAULT_CUSTOM_OPENAI_MODEL);
                    setModelConfig((prev: ModelConfig) => ({
                      ...prev,
                      provider,
                      model: DEFAULT_CUSTOM_OPENAI_MODEL,
                      customOpenAIEndpoint: DEFAULT_CUSTOM_OPENAI_ENDPOINT,
                      customOpenAIModel: DEFAULT_CUSTOM_OPENAI_MODEL,
                    }));
                  });
                }
              }}
            >
              <SelectTrigger>
                <SelectValue placeholder="Chọn nhà cung cấp" />
              </SelectTrigger>
              <SelectContent className="max-h-64 overflow-y-auto">
                <SelectItem value="claude">Claude</SelectItem>
                <SelectItem value="custom-openai">Máy chủ tùy chỉnh (OpenAI)</SelectItem>
                <SelectItem value="openai">OpenAI</SelectItem>
                <SelectItem value="openrouter">OpenRouter</SelectItem>
              </SelectContent>
            </Select>

            {modelConfig.provider !== 'custom-openai' && (
              <Popover open={modelComboboxOpen} onOpenChange={setModelComboboxOpen} modal={true}>
                <PopoverTrigger asChild>
                  <Button
                    variant="outline"
                    role="combobox"
                    aria-expanded={modelComboboxOpen}
                    className="flex-1 max-w-[200px] justify-between font-normal"
                    disabled={!hasModelsForProvider}
                  >
                    <span className="truncate">
                      {selectedModelLabel}
                    </span>
                    <ChevronsUpDown className="ml-2 h-4 w-4 shrink-0 opacity-50" />
                  </Button>
                </PopoverTrigger>
                <PopoverContent className="w-[250px] p-0" align="start">
                  <Command>
                    <CommandInput placeholder="Tìm mô hình…" />
                    <CommandList className="max-h-[300px]">
                      {(modelConfig.provider === 'openrouter' && isLoadingOpenRouter) ||
                       (modelConfig.provider === 'openai' && isLoadingOpenAI) ||
                       (modelConfig.provider === 'claude' && isLoadingClaude) ? (
                        <div className="py-6 text-center text-sm text-muted-foreground">
                          <RefreshCw className="mx-auto h-4 w-4 animate-spin mb-2" />
                          Đang tải mô hình...
                        </div>
                      ) : (
                        <>
                          <CommandEmpty>Không có mô hình.</CommandEmpty>
                          <CommandGroup>
                            {modelOptions[modelConfig.provider]?.map((model) => (
                              <CommandItem
                                key={model}
                                value={model}
                                onSelect={(currentValue) => {
                                  setModelConfig((prev: ModelConfig) => ({ ...prev, model: currentValue }));
                                  setModelComboboxOpen(false);
                                }}
                              >
                                <Check
                                  className={cn(
                                    "mr-2 h-4 w-4",
                                    modelConfig.model === model ? "opacity-100" : "opacity-0"
                                  )}
                                />
                                <span className="truncate">{model}</span>
                              </CommandItem>
                            ))}
                          </CommandGroup>
                        </>
                      )}
                    </CommandList>
                  </Command>
                </PopoverContent>
              </Popover>
            )}
          </div>
        </div>

        {/* Fallback Models Section — shown for providers with multiple models */}
        {(['openai', 'claude', 'openrouter'] as const).includes(modelConfig.provider as any) &&
          (modelOptions[modelConfig.provider]?.length ?? 0) > 1 && (
          <div className="space-y-2 border-t pt-3">
            <Label className="text-sm font-medium">
              Model dự phòng
              <span className="ml-1 text-xs text-muted-foreground font-normal">
                (tự chuyển sang khi model chính bị rate limit)
              </span>
            </Label>
            <div className="relative">
              <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground pointer-events-none" />
              <Input
                placeholder="Tìm model dự phòng…"
                value={fallbackSearchQuery}
                onChange={(e) => setFallbackSearchQuery(e.target.value)}
                className="h-7 pl-7 text-sm"
              />
            </div>
            <div className="max-h-36 overflow-y-auto space-y-1 rounded border p-2 bg-muted/30">
              {(modelOptions[modelConfig.provider] ?? [])
                .filter((m) => m !== modelConfig.model)
                .filter((m) =>
                  !fallbackSearchQuery.trim() ||
                  m.toLowerCase().includes(fallbackSearchQuery.toLowerCase())
                )
                .map((model) => (
                  <div key={model} className="flex items-center gap-2 py-0.5">
                    <input
                      type="checkbox"
                      id={`fallback-${model}`}
                      checked={fallbackModels.includes(model)}
                      onChange={(e) =>
                        setFallbackModels((prev) =>
                          e.target.checked
                            ? [...prev, model]
                            : prev.filter((m) => m !== model)
                        )
                      }
                      className="h-3.5 w-3.5 cursor-pointer accent-[#16478e]"
                    />
                    <label
                      htmlFor={`fallback-${model}`}
                      className="text-sm cursor-pointer truncate"
                    >
                      {model}
                    </label>
                  </div>
                ))
              }
            </div>
            {fallbackModels.length > 0 && (
              <p className="text-xs text-muted-foreground">
                Thứ tự thử: {[modelConfig.model, ...fallbackModels].join(' → ')}
              </p>
            )}
          </div>
        )}

        {/* Custom OpenAI Configuration Section */}
        {modelConfig.provider === 'custom-openai' && (
          <div className="space-y-4 border-t pt-4">
            <div>
              <Label htmlFor="custom-endpoint">URL endpoint *</Label>
              <Input
                id="custom-endpoint"
                value={customOpenAIEndpoint}
                onChange={(e) => setCustomOpenAIEndpoint(e.target.value)}
                placeholder="http://localhost:8000/v1"
                className="mt-1"
              />
              <p className="text-xs text-muted-foreground mt-1">
                Địa chỉ gốc của API tương thích OpenAI
              </p>
            </div>

            <div>
              <Label htmlFor="custom-model">Tên mô hình *</Label>
              <Input
                id="custom-model"
                value={customOpenAIModel}
                onChange={(e) => setCustomOpenAIModel(e.target.value)}
                placeholder="ví dụ: gpt-4, llama-3-70b"
                className="mt-1"
              />
              <p className="text-xs text-muted-foreground mt-1">
                Định danh mô hình dùng cho mỗi yêu cầu
              </p>
            </div>

            <div>
              <Label htmlFor="custom-api-key">API Key (tuỳ chọn)</Label>
              <Input
                id="custom-api-key"
                type="password"
                value={customOpenAIApiKey}
                onChange={(e) => setCustomOpenAIApiKey(e.target.value)}
                placeholder="Để trống nếu không bắt buộc"
                className="mt-1"
              />
            </div>

            {/* Advanced Options (Collapsible) */}
            <div>
              <div
                className="flex items-center justify-between cursor-pointer py-2"
                onClick={() => setIsCustomOpenAIAdvancedOpen(!isCustomOpenAIAdvancedOpen)}
              >
                <Label className="cursor-pointer">Tùy chọn nâng cao</Label>
                {isCustomOpenAIAdvancedOpen ? (
                  <ChevronUp className="h-4 w-4 text-muted-foreground" />
                ) : (
                  <ChevronDown className="h-4 w-4 text-muted-foreground" />
                )}
              </div>

              {isCustomOpenAIAdvancedOpen && (
                <div className="space-y-3 pl-2 border-l-2 border-muted mt-2">
                  <div>
                    <Label htmlFor="custom-max-tokens">Số token tối đa</Label>
                    <Input
                      id="custom-max-tokens"
                      type="number"
                      value={customMaxTokens}
                      onChange={(e) => setCustomMaxTokens(e.target.value)}
                      placeholder="ví dụ: 4096"
                      className="mt-1"
                    />
                  </div>
                  <div>
                    <Label htmlFor="custom-temperature">Nhiệt độ (0.0–2.0)</Label>
                    <Input
                      id="custom-temperature"
                      type="number"
                      step="0.1"
                      min="0"
                      max="2"
                      value={customTemperature}
                      onChange={(e) => setCustomTemperature(e.target.value)}
                      placeholder="ví dụ: 0.7"
                      className="mt-1"
                    />
                  </div>
                  <div>
                    <Label htmlFor="custom-top-p">Top P (0.0–1.0)</Label>
                    <Input
                      id="custom-top-p"
                      type="number"
                      step="0.1"
                      min="0"
                      max="1"
                      value={customTopP}
                      onChange={(e) => setCustomTopP(e.target.value)}
                      placeholder="ví dụ: 0.9"
                      className="mt-1"
                    />
                  </div>
                </div>
              )}
            </div>

            {/* Test Connection Button */}
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={testCustomOpenAIConnection}
              disabled={isTestingConnection || !customOpenAIEndpoint.trim() || !customOpenAIModel.trim()}
              className="w-full"
            >
              {isTestingConnection ? (
                <>
                  <RefreshCw className="mr-2 h-4 w-4 animate-spin" />
                  Đang thử kết nối…
                </>
              ) : (
                <>
                  <CheckCircle2 className="mr-2 h-4 w-4" />
                  Thử kết nối
                </>
              )}
            </Button>
          </div>
        )}

        {requiresApiKey && (
          <div>
            <Label>Khóa API</Label>
            <div className="relative mt-1">
              <Input
                type={showApiKey ? 'text' : 'password'}
                value={apiKey || ''}
                onChange={(e) => setApiKey(e.target.value)}
                disabled={isApiKeyLocked}
                placeholder="Nhập khóa API"
                className="pr-24"
              />
              {isApiKeyLocked && apiKey?.trim() && (
                <div
                  onClick={handleInputClick}
                  className="absolute inset-0 flex items-center justify-center bg-muted/50 rounded-md cursor-not-allowed"
                />
              )}
              <div className="absolute inset-y-0 right-0 pr-1 flex items-center space-x-1">
                {apiKey?.trim() && (
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    onClick={() => setIsApiKeyLocked(!isApiKeyLocked)}
                    className={isLockButtonVibrating ? 'animate-vibrate text-red-500' : ''}
                    title={isApiKeyLocked ? 'Mở khóa để chỉnh sửa' : 'Khóa để tránh chỉnh nhầm'}
                  >
                    {isApiKeyLocked ? <Lock /> : <Unlock />}
                  </Button>
                )}
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  onClick={() => setShowApiKey(!showApiKey)}
                >
                  {showApiKey ? <EyeOff /> : <Eye />}
                </Button>
              </div>
            </div>
          </div>
        )}

      </div>

      {!embedded && (
        <div className="mt-6 flex justify-end">
          <Button
            className={cn(
              'px-4 text-sm font-medium text-white rounded-md focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-[#16478e]',
              isDoneDisabled ? 'bg-gray-400 cursor-not-allowed' : 'bg-[#16478e] hover:bg-[#1a55ab]'
            )}
            onClick={handleSave}
            disabled={isDoneDisabled}
          >
            Lưu
          </Button>
        </div>
      )}
    </div>
  );
});
