import unittest

import numpy as np
import torch

from g3_evaluate import values
from g3_train import forecast_labels


class G3Tests(unittest.TestCase):
    def test_forecasts_never_cross_resets_or_use_rollout_tail(self):
        obs = torch.ones(14, 2, 48)
        obs[8:, 0, 0] = .8
        obs[8:, 1, 1] = .7
        done = np.zeros((14, 2), dtype=bool)
        done[7, 1] = True
        labels, known = forecast_labels(obs, done)
        labels, known = labels.reshape(14, 2, 2), known.reshape(14, 2)
        self.assertEqual(labels[0, 0].tolist(), [1., 0.])
        self.assertTrue(known[:6, 0].all())
        self.assertFalse(known[:, 1].any())
        self.assertFalse(known[6:].any())

    def test_openings_require_profitable_distinct_windows(self):
        ex = dict(windows=3, even=1, repeated_even=0, longest_even_streak=1, clean=2, profitable_openings=[1, 0, 2, 0, 0, 0])
        result = values(dict(outcome=3, hp=10, exchanges=ex))
        np.testing.assert_allclose(result, [0, 0, 10, 1 / 3, 2 / 3, 2 / 3, 1, 0, 1])
        ex.update(windows=0, even=0, clean=0, longest_even_streak=0, profitable_openings=[0] * 6)
        np.testing.assert_allclose(values(dict(outcome=1, hp=4.5, exchanges=ex)), [1, 4.5, 4.5, 0, 0, 0, 0, 0, 0])


if __name__ == "__main__":
    unittest.main()
