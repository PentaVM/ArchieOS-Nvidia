import importlib.util
from pathlib import Path
import sys
import unittest

SOURCE = Path(__file__).resolve().parents[1] / 'analyze.py'


def module():
    spec = importlib.util.spec_from_file_location('sentry_bridge_analysis', SOURCE)
    result = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = result
    spec.loader.exec_module(result)
    return result


class WanEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.m = module()
        self.summary = self.m.WanSummary(['192.168.1.107', '2606:4700::1111'])

    def test_counts_both_directions_and_ignores_lan(self):
        self.summary.observe({'ip.src':'192.168.1.107','ip.dst':'1.1.1.1','frame.len':'100','tcp.dstport':'443'})
        self.summary.observe({'ip.src':'1.1.1.1','ip.dst':'192.168.1.107','frame.len':'240','tcp.srcport':'443'})
        self.summary.observe({'ip.src':'192.168.1.107','ip.dst':'192.168.1.1','frame.len':'80'})
        result = self.summary.result()
        self.assertEqual(result['outbound_bytes'], 100)
        self.assertEqual(result['inbound_bytes'], 240)
        self.assertEqual(result['wan_frames'], 2)
        self.assertFalse(result['gateway_wan_observed'])

    def test_public_ipv6_is_not_lost_as_non_lan(self):
        self.summary.observe({'ipv6.src':'2606:4700::1111','ipv6.dst':'2001:4860:4860::8888','frame.len':'128'})
        self.assertEqual(self.summary.result()['outbound_bytes'], 128)

    def test_neighbor_traffic_does_not_become_host_wan(self):
        self.summary.observe({'ip.src':'192.168.1.42','ip.dst':'8.8.8.8','frame.len':'100'})
        self.assertEqual(self.summary.result()['wan_frames'], 0)

    def test_global_ipv6_on_link_neighbor_is_excluded(self):
        summary = self.m.WanSummary(['2606:4700::1111'], ['2606:4700::/64'])
        summary.observe({'ipv6.src':'2606:4700::1111','ipv6.dst':'2606:4700::2222','frame.len':'100'})
        summary.observe({'ipv6.src':'2606:4700::2222','ipv6.dst':'2606:4700::1111','frame.len':'100'})
        self.assertEqual(summary.result()['wan_frames'], 0)
        summary.observe({'ipv6.src':'2606:4700::1111','ipv6.dst':'2001:4860:4860::8888','frame.len':'100'})
        self.assertEqual(summary.result()['wan_frames'], 1)

    def test_multicast_discovery_is_not_internet_traffic(self):
        self.summary.observe({'ip.src':'192.168.1.107','ip.dst':'224.0.0.251','frame.len':'100'})
        self.summary.observe({'ipv6.src':'2606:4700::1111','ipv6.dst':'ff02::fb','frame.len':'100'})
        self.assertEqual(self.summary.result()['wan_frames'], 0)

    def test_peer_cardinality_is_bounded_without_losing_byte_totals(self):
        for i in range(200):
            self.summary.observe({'ip.src':'192.168.1.107','ip.dst':f'8.8.0.{i+1}','frame.len':'100'})
        result = self.summary.result()
        self.assertLessEqual(len(result['peers']), 128)
        self.assertTrue(result['peer_tracking_truncated'])
        self.assertEqual(result['outbound_bytes'], 20000)

    def test_does_not_invent_stable_identity_when_pcap_lacks_names(self):
        a, stable_a = self.m.interface_key('capture-a', '0', '', '')
        b, stable_b = self.m.interface_key('capture-b', '0', '', '')
        self.assertNotEqual(a, b)
        self.assertFalse(stable_a or stable_b)

    def test_stable_interface_identity_survives_window_reindex(self):
        a, stable_a = self.m.interface_key('a', '0', 'Ethernet', 'Realtek')
        b, stable_b = self.m.interface_key('b', '7', 'Ethernet', 'Realtek')
        self.assertEqual(a, b)
        self.assertTrue(stable_a and stable_b)

    def test_only_classic_single_interface_pcap_may_infer_missing_id(self):
        self.assertEqual(self.m.normalize_interface_id('', True), '0')
        with self.assertRaises(ValueError):
            self.m.normalize_interface_id('', False)

    def test_unknown_pcapng_placeholder_is_not_stable_provenance(self):
        self.assertEqual(self.m.observation_identity('0', 'unknown', '', None), ('0', '', ''))

    def test_filtered_component_uses_windows_guid_not_unknown_interface(self):
        component = {'id':11, 'adapter_name':'Ethernet', 'adapter_guid':'11111111-2222-4333-8444-555555555555'}
        identity = self.m.observation_identity('0', 'unknown', '', component)
        self.assertEqual(identity, ('0', 'Ethernet', 'windows-pktmon:11111111-2222-4333-8444-555555555555'))


if __name__ == '__main__':
    unittest.main()
