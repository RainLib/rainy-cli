import { createTemplateAction } from '@backstage/plugin-scaffolder-node';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);

type RainyActionOptions = {
  rainyBin?: string;
};

export function createRainyInitAppAction(options: RainyActionOptions = {}) {
  return createTemplateAction<{
    name: string;
    preset?: string;
    package?: string;
  }>({
    id: 'rainy:init-app',
    description: 'Initialize a Rainy application.',
    schema: {
      input: {
        type: 'object',
        required: ['name'],
        properties: {
          name: { type: 'string' },
          preset: { type: 'string', default: 'spring-nextjs' },
          package: { type: 'string', default: 'com.example.demo' },
        },
      },
    },
    async handler(ctx) {
      await rainy(options, ctx.workspacePath, [
        'init',
        'app',
        ctx.input.name,
        '--preset',
        ctx.input.preset ?? 'spring-nextjs',
        '--package',
        ctx.input.package ?? 'com.example.demo',
        '--json',
      ]);
    },
  });
}

export function createRainyAddCapabilityAction(options: RainyActionOptions = {}) {
  return createTemplateAction<{
    capability: string;
    provider?: string;
    apply?: boolean;
  }>({
    id: 'rainy:add-capability',
    description: 'Add a Rainy capability.',
    schema: {
      input: {
        type: 'object',
        required: ['capability'],
        properties: {
          capability: { type: 'string' },
          provider: { type: 'string' },
          apply: { type: 'boolean', default: false },
        },
      },
    },
    async handler(ctx) {
      const args = ['add', 'capability', ctx.input.capability, '--json'];
      if (ctx.input.provider) args.push('--provider', ctx.input.provider);
      args.push(ctx.input.apply ? '--apply' : '--dry-run');
      await rainy(options, ctx.workspacePath, args);
    },
  });
}

export function createRainyDoctorAction(options: RainyActionOptions = {}) {
  return createTemplateAction<{ capability?: string }>({
    id: 'rainy:doctor',
    description: 'Run Rainy doctor.',
    schema: {
      input: {
        type: 'object',
        properties: {
          capability: { type: 'string' },
        },
      },
    },
    async handler(ctx) {
      const args = ['doctor', '--json'];
      if (ctx.input.capability) args.push('--capability', ctx.input.capability);
      await rainy(options, ctx.workspacePath, args);
    },
  });
}

export function createRainyEvidenceAction(options: RainyActionOptions = {}) {
  return createTemplateAction<{ format?: 'markdown' | 'json' | 'all' }>({
    id: 'rainy:evidence',
    description: 'Generate Rainy evidence.',
    schema: {
      input: {
        type: 'object',
        properties: {
          format: { type: 'string', enum: ['markdown', 'json', 'all'] },
        },
      },
    },
    async handler(ctx) {
      await rainy(options, ctx.workspacePath, [
        'evidence',
        'generate',
        '--format',
        ctx.input.format ?? 'all',
        '--json',
      ]);
    },
  });
}

async function rainy(options: RainyActionOptions, workspacePath: string, args: string[]) {
  const rainyBin = options.rainyBin ?? process.env.RAINY_BIN ?? 'rainy';
  await execFileAsync(rainyBin, ['--workspace', workspacePath, ...args], {
    cwd: workspacePath,
  });
}
