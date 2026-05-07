import { defineCollection, z } from 'astro:content';
import { glob } from 'astro/loaders';

// Blog collection. Markdown files live in ./src/content/blog/*.md and the
// frontmatter is validated against this schema at build time. Adding a new
// post is a one-file change.
const blog = defineCollection({
  loader: glob({ pattern: '**/*.md', base: './src/content/blog' }),
  schema: z.object({
    title: z.string(),
    description: z.string(),
    publishedAt: z.coerce.date(),
    updatedAt: z.coerce.date().optional(),
    author: z.string().default('Luuk Dahlmans'),
    tags: z.array(z.string()).default([]),
    keywords: z.array(z.string()).default([]),
    draft: z.boolean().default(false),
  }),
});

export const collections = { blog };
