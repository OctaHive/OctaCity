import { configure } from '@testing-library/react';

// Async route chunks and authoritative reads can settle while other repository checks run.
configure({ asyncUtilTimeout: 5_000 });
